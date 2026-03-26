use anyhow::Result;
use rand::Rng;
use rusqlite::Connection;
use std::time::Instant;
use zerocopy::AsBytes;

pub struct BenchResult {
    pub label: String,
    pub p50_us: u128,
    pub p95_us: u128,
    pub p99_us: u128,
    pub avg_us: u128,
}

impl std::fmt::Display for BenchResult {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{:<40} avg={:>7}us  p50={:>7}us  p95={:>7}us  p99={:>7}us",
            self.label, self.avg_us, self.p50_us, self.p95_us, self.p99_us
        )
    }
}

fn percentile(sorted: &[u128], p: f64) -> u128 {
    let idx = ((sorted.len() as f64) * p / 100.0).ceil() as usize;
    sorted[idx.min(sorted.len() - 1)]
}

fn random_normalized_vec(dim: usize) -> Vec<f32> {
    let mut rng = rand::thread_rng();
    let vec: Vec<f32> = (0..dim).map(|_| rng.gen_range(-1.0f32..1.0f32)).collect();
    let norm: f32 = vec.iter().map(|x| x * x).sum::<f32>().sqrt();
    vec.iter().map(|x| x / norm).collect()
}

pub fn bench_knn(conn: &Connection, dim: usize, k: usize, num_queries: usize) -> Result<BenchResult> {
    let mut latencies = Vec::with_capacity(num_queries);

    let mut stmt = conn.prepare(
        "SELECT summary_id, distance
         FROM summary_embeddings
         WHERE embedding MATCH ?1
           AND k = ?2
         ORDER BY distance",
    )?;

    for _ in 0..num_queries {
        let query_vec = random_normalized_vec(dim);
        let start = Instant::now();
        let rows: Vec<(i64, f64)> = stmt
            .query_map(rusqlite::params![query_vec.as_bytes(), k as i64], |row| {
                Ok((row.get(0)?, row.get(1)?))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        let elapsed = start.elapsed().as_micros();
        latencies.push(elapsed);
        // Consume rows to ensure full execution
        let _ = rows.len();
    }

    latencies.sort();
    let avg = latencies.iter().sum::<u128>() / latencies.len() as u128;

    Ok(BenchResult {
        label: format!("KNN (dim={}, k={})", dim, k),
        p50_us: percentile(&latencies, 50.0),
        p95_us: percentile(&latencies, 95.0),
        p99_us: percentile(&latencies, 99.0),
        avg_us: avg,
    })
}

pub fn bench_knn_with_join(conn: &Connection, dim: usize, k: usize, num_queries: usize) -> Result<BenchResult> {
    let mut latencies = Vec::with_capacity(num_queries);

    let mut stmt = conn.prepare(
        "SELECT s.id, s.summary, s.level, e.distance
         FROM summary_embeddings e
         JOIN summaries s ON s.id = e.summary_id
         WHERE e.embedding MATCH ?1
           AND k = ?2
         ORDER BY e.distance",
    )?;

    for _ in 0..num_queries {
        let query_vec = random_normalized_vec(dim);
        let start = Instant::now();
        let rows: Vec<(i64, String, String, f64)> = stmt
            .query_map(rusqlite::params![query_vec.as_bytes(), k as i64], |row| {
                Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        let elapsed = start.elapsed().as_micros();
        latencies.push(elapsed);
        let _ = rows.len();
    }

    latencies.sort();
    let avg = latencies.iter().sum::<u128>() / latencies.len() as u128;

    Ok(BenchResult {
        label: format!("KNN+JOIN (dim={}, k={})", dim, k),
        p50_us: percentile(&latencies, 50.0),
        p95_us: percentile(&latencies, 95.0),
        p99_us: percentile(&latencies, 99.0),
        avg_us: avg,
    })
}

pub fn bench_hybrid_app_side(
    conn: &Connection,
    dim: usize,
    k: usize,
    num_queries: usize,
) -> Result<BenchResult> {
    let mut latencies = Vec::with_capacity(num_queries);
    let mut rng = rand::thread_rng();
    let max_file_id: i64 = conn.query_row("SELECT MAX(id) FROM files", [], |r| r.get(0))?;

    for _ in 0..num_queries {
        let start = Instant::now();

        // Step 1: structural query to get candidate file IDs
        let changed_file_id: i64 = rng.gen_range(1..=max_file_id);
        let mut struct_stmt = conn.prepare_cached(
            "SELECT DISTINCT f2.id FROM imports i
             JOIN files f2 ON f2.path = i.source_path
             WHERE i.file_id = ?1
             UNION
             SELECT file_id_b FROM cochange WHERE file_id_a = ?1 AND correlation > 0.3",
        )?;
        let related_ids: Vec<i64> = struct_stmt
            .query_map([changed_file_id], |row| row.get(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?;

        // Step 2: KNN query
        let query_vec = random_normalized_vec(dim);
        let mut knn_stmt = conn.prepare_cached(
            "SELECT summary_id, distance
             FROM summary_embeddings
             WHERE embedding MATCH ?1
               AND k = ?2
             ORDER BY distance",
        )?;
        let knn_results: Vec<(i64, f64)> = knn_stmt
            .query_map(rusqlite::params![query_vec.as_bytes(), (k * 5) as i64], |row| {
                Ok((row.get(0)?, row.get(1)?))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;

        // Step 3: filter KNN results to structural candidates
        // In real usage we'd join summary_id -> symbol -> file_id.
        // Here we approximate by checking if summary_id (which maps 1:1 to symbols)
        // falls within the file range.
        let _hybrid: Vec<_> = knn_results
            .into_iter()
            .filter(|(sid, _)| {
                // Approximate: symbol belongs to file if file_id matches
                let file_id = (sid - 1) / 10 + 1; // 10 symbols per file
                related_ids.contains(&file_id)
            })
            .take(k)
            .collect();

        let elapsed = start.elapsed().as_micros();
        latencies.push(elapsed);
    }

    latencies.sort();
    let avg = latencies.iter().sum::<u128>() / latencies.len() as u128;

    Ok(BenchResult {
        label: format!("Hybrid/app-side (dim={}, k={})", dim, k),
        p50_us: percentile(&latencies, 50.0),
        p95_us: percentile(&latencies, 95.0),
        p99_us: percentile(&latencies, 99.0),
        avg_us: avg,
    })
}

pub fn bench_insert(conn: &Connection, count: usize, dim: usize) -> Result<(std::time::Duration, u64)> {
    // Drop and recreate to measure clean insert
    conn.execute("DROP TABLE IF EXISTS bench_insert_embeddings", [])?;
    conn.execute_batch(&format!(
        "CREATE VIRTUAL TABLE bench_insert_embeddings USING vec0(
            id INTEGER PRIMARY KEY,
            embedding float[{dim}]
        );"
    ))?;

    let mut rng = rand::thread_rng();
    let start = Instant::now();

    let mut stmt = conn.prepare(
        "INSERT INTO bench_insert_embeddings (id, embedding) VALUES (?1, ?2)",
    )?;
    for i in 1..=count as i64 {
        let vec: Vec<f32> = (0..dim).map(|_| rng.gen_range(-1.0f32..1.0f32)).collect();
        let norm: f32 = vec.iter().map(|x| x * x).sum::<f32>().sqrt();
        let vec: Vec<f32> = vec.iter().map(|x| x / norm).collect();
        stmt.execute(rusqlite::params![i, vec.as_bytes()])?;
    }

    let elapsed = start.elapsed();

    // Clean up
    conn.execute("DROP TABLE bench_insert_embeddings", [])?;

    // Get DB file size (approximate from page count)
    let page_count: i64 = conn.query_row("PRAGMA page_count", [], |r| r.get(0))?;
    let page_size: i64 = conn.query_row("PRAGMA page_size", [], |r| r.get(0))?;
    let db_size = (page_count * page_size) as u64;

    Ok((elapsed, db_size))
}
