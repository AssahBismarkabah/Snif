mod bench;
mod populate;
mod schema;

use anyhow::Result;
use rusqlite::{ffi::sqlite3_auto_extension, Connection};
use sqlite_vec::sqlite3_vec_init;
use std::time::Instant;
use zerocopy::AsBytes;

fn init_sqlite_vec() {
    unsafe {
        sqlite3_auto_extension(Some(std::mem::transmute(sqlite3_vec_init as *const ())));
    }
}

fn run_spike(db_path: &str, dim: usize, scale: &populate::Scale) -> Result<()> {
    println!("\n{}", "=".repeat(60));
    println!(
        "  Scale: {} ({} files, {} embeddings, dim={})",
        scale.name,
        scale.num_files,
        scale.num_files * scale.symbols_per_file,
        dim
    );
    println!("{}\n", "=".repeat(60));

    // Clean start
    let _ = std::fs::remove_file(db_path);
    let conn = Connection::open(db_path)?;
    conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA synchronous=NORMAL;")?;

    // Create schema
    schema::create_schema(&conn)?;
    schema::create_vec_tables(&conn, dim)?;

    // Populate structural data
    let start = Instant::now();
    populate::populate_structural(&conn, scale)?;
    let struct_time = start.elapsed();
    println!("  Structural data populated in {:?}", struct_time);

    // Populate embeddings
    let embed_count = scale.num_files * scale.symbols_per_file;
    let start = Instant::now();
    populate::populate_embeddings(&conn, embed_count, dim)?;
    let embed_time = start.elapsed();
    println!(
        "  {} embeddings inserted in {:?} ({:.0} inserts/sec)",
        embed_count,
        embed_time,
        embed_count as f64 / embed_time.as_secs_f64()
    );

    // DB file size
    let file_size = std::fs::metadata(db_path)?.len();
    println!("  DB file size: {:.1} MB", file_size as f64 / 1_048_576.0);

    // Benchmarks
    println!("\n  --- KNN Benchmarks (100 queries, k=20) ---\n");

    let result = bench::bench_knn(&conn, dim, 20, 100)?;
    println!("  {}", result);

    let result = bench::bench_knn_with_join(&conn, dim, 20, 100)?;
    println!("  {}", result);

    println!("\n  --- Hybrid Query Benchmarks (50 queries, k=20) ---\n");

    let result = bench::bench_hybrid_app_side(&conn, dim, 20, 50)?;
    println!("  {}", result);

    // Insert throughput benchmark
    println!("\n  --- Insert Throughput ---\n");
    let (duration, _) = bench::bench_insert(&conn, 10_000, dim)?;
    println!(
        "  10k inserts (dim={}): {:?} ({:.0} inserts/sec)",
        dim,
        duration,
        10_000.0 / duration.as_secs_f64()
    );

    Ok(())
}

fn main() -> Result<()> {
    init_sqlite_vec();

    // Step 1: Verify sqlite-vec loads correctly
    println!("=== Spike 1: SQLite + sqlite-vec Validation ===\n");
    {
        let db = Connection::open_in_memory()?;
        let (version, round_trip): (String, String) = db.query_row(
            "SELECT vec_version(), vec_to_json(?)",
            &[vec![0.1f32, 0.2, 0.3, 0.4].as_bytes()],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?;
        println!("  sqlite-vec version: {}", version);
        println!("  Vector round-trip:  {}", round_trip);
        println!("  Bootstrap: PASS\n");
    }

    // Step 2: Run at each scale with 384 dimensions
    let db_path = "/tmp/snif_spike_sqlite_vec.db";

    run_spike(db_path, 384, &populate::SMALL)?;
    run_spike(db_path, 384, &populate::MEDIUM)?;
    run_spike(db_path, 384, &populate::LARGE)?;

    // Step 3: Dimension comparison at MEDIUM scale
    println!("\n{}", "=".repeat(60));
    println!("  Dimension Comparison (medium scale, 25k embeddings)");
    println!("{}\n", "=".repeat(60));

    for dim in [384, 768, 1536] {
        let path = format!("/tmp/snif_spike_dim_{}.db", dim);
        let _ = std::fs::remove_file(&path);
        let conn = Connection::open(&path)?;
        conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA synchronous=NORMAL;")?;
        schema::create_schema(&conn)?;
        schema::create_vec_tables(&conn, dim)?;
        populate::populate_structural(&conn, &populate::MEDIUM)?;
        populate::populate_embeddings(&conn, 25_000, dim)?;

        let file_size = std::fs::metadata(&path)?.len();
        let result = bench::bench_knn(&conn, dim, 20, 100)?;

        println!(
            "  dim={:>4}  db={:>6.1}MB  {}",
            dim,
            file_size as f64 / 1_048_576.0,
            result
        );

        let _ = std::fs::remove_file(&path);
    }

    // Cleanup
    let _ = std::fs::remove_file(db_path);

    println!("\n=== Spike 1 Complete ===");
    Ok(())
}
