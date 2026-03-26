use anyhow::Result;
use rand::Rng;
use rusqlite::Connection;
use zerocopy::AsBytes;

pub struct Scale {
    pub name: &'static str,
    pub num_files: usize,
    pub symbols_per_file: usize,
    pub imports_per_file: usize,
}

pub const SMALL: Scale = Scale {
    name: "small",
    num_files: 500,
    symbols_per_file: 10,
    imports_per_file: 4,
};

pub const MEDIUM: Scale = Scale {
    name: "medium",
    num_files: 2500,
    symbols_per_file: 10,
    imports_per_file: 4,
};

pub const LARGE: Scale = Scale {
    name: "large",
    num_files: 5000,
    symbols_per_file: 10,
    imports_per_file: 4,
};

pub fn populate_structural(conn: &Connection, scale: &Scale) -> Result<()> {
    let mut rng = rand::thread_rng();

    // Insert files
    let mut file_stmt = conn.prepare(
        "INSERT INTO files (path, hash, language) VALUES (?1, ?2, ?3)",
    )?;
    for i in 0..scale.num_files {
        let module = i / 50;
        let path = format!("src/module_{}/file_{}.rs", module, i);
        let hash = format!("{:016x}", rng.gen::<u64>());
        file_stmt.execute(rusqlite::params![path, hash, "rust"])?;
    }

    // Insert symbols
    let mut sym_stmt = conn.prepare(
        "INSERT INTO symbols (file_id, name, kind, start_line, end_line, signature)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
    )?;
    for file_id in 1..=scale.num_files as i64 {
        for s in 0..scale.symbols_per_file {
            let kind = match s % 4 {
                0 => "function",
                1 => "struct",
                2 => "enum",
                _ => "trait",
            };
            let line_start = s * 20;
            sym_stmt.execute(rusqlite::params![
                file_id,
                format!("symbol_{}_{}", file_id, s),
                kind,
                line_start,
                line_start + 15,
                format!("fn symbol_{}_{}() -> Result<()>", file_id, s),
            ])?;
        }
    }

    // Insert imports (random edges between files)
    let mut import_stmt = conn.prepare(
        "INSERT INTO imports (file_id, source_path, kind) VALUES (?1, ?2, ?3)",
    )?;
    for file_id in 1..=scale.num_files as i64 {
        for _ in 0..scale.imports_per_file {
            let target_file = rng.gen_range(0..scale.num_files);
            let module = target_file / 50;
            let path = format!("src/module_{}/file_{}.rs", module, target_file);
            import_stmt.execute(rusqlite::params![file_id, path, "direct"])?;
        }
    }

    // Insert cochange pairs (sample 2 correlations per file)
    let mut cochange_stmt = conn.prepare(
        "INSERT OR IGNORE INTO cochange (file_id_a, file_id_b, correlation, commit_count)
         VALUES (?1, ?2, ?3, ?4)",
    )?;
    for file_id in 1..=scale.num_files as i64 {
        for _ in 0..2 {
            let other = rng.gen_range(1..=scale.num_files as i64);
            if other != file_id {
                let corr: f64 = rng.gen_range(0.1..1.0);
                let commits = rng.gen_range(3..50);
                cochange_stmt.execute(rusqlite::params![file_id, other, corr, commits])?;
            }
        }
    }

    // Insert summaries (one per symbol)
    let mut sum_stmt = conn.prepare(
        "INSERT INTO summaries (symbol_id, level, summary, token_count)
         VALUES (?1, ?2, ?3, ?4)",
    )?;
    let total_symbols = scale.num_files * scale.symbols_per_file;
    for sym_id in 1..=total_symbols as i64 {
        sum_stmt.execute(rusqlite::params![
            sym_id,
            "function",
            format!("This function handles processing for symbol {}.", sym_id),
            rng.gen_range(20..60),
        ])?;
    }

    Ok(())
}

pub fn populate_embeddings(conn: &Connection, count: usize, dim: usize) -> Result<()> {
    let mut rng = rand::thread_rng();
    let mut stmt = conn.prepare(
        "INSERT INTO summary_embeddings (summary_id, embedding) VALUES (?1, ?2)",
    )?;

    for i in 1..=count as i64 {
        let vec: Vec<f32> = (0..dim).map(|_| rng.gen_range(-1.0f32..1.0f32)).collect();
        // Normalize
        let norm: f32 = vec.iter().map(|x| x * x).sum::<f32>().sqrt();
        let vec: Vec<f32> = vec.iter().map(|x| x / norm).collect();
        stmt.execute(rusqlite::params![i, vec.as_bytes()])?;
    }

    Ok(())
}
