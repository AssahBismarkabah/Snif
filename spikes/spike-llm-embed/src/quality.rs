use anyhow::Result;
use rusqlite::Connection;
use zerocopy::AsBytes;

pub struct SimilarityResult {
    pub query_name: String,
    pub query_file: String,
    pub matches: Vec<MatchResult>,
}

pub struct MatchResult {
    pub name: String,
    pub file_path: String,
    pub distance: f64,
    pub summary: String,
}

pub fn query_similar(
    conn: &Connection,
    query_embedding: &[f32],
    k: usize,
) -> Result<Vec<MatchResult>> {
    let mut stmt = conn.prepare(
        "SELECT s.id, s.name, s.file_path, s.summary, e.distance
         FROM summary_embeddings e
         JOIN code_summaries s ON s.id = e.summary_id
         WHERE e.embedding MATCH ?1
           AND k = ?2
         ORDER BY e.distance",
    )?;

    let results: Vec<MatchResult> = stmt
        .query_map(rusqlite::params![query_embedding.as_bytes(), k as i64], |row| {
            Ok(MatchResult {
                name: row.get(1)?,
                file_path: row.get(2)?,
                distance: row.get(4)?,
                summary: row.get(3)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;

    Ok(results)
}

fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        format!("{}...", &s[..max])
    }
}

pub fn print_similarity_results(results: &[SimilarityResult]) {
    println!("\n  --- Embedding Quality: Top-10 Similar ---\n");

    for result in results {
        println!(
            "  Query: {} ({})",
            result.query_name, result.query_file
        );
        for (i, m) in result.matches.iter().enumerate() {
            println!(
                "    {:>2}. [{:.3}] {} ({}) - {}",
                i + 1,
                m.distance,
                m.name,
                m.file_path,
                truncate(&m.summary, 60)
            );
        }
        println!();
    }
}
