use anyhow::Result;
use rusqlite::Connection;

pub fn create_schema(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "
        -- Structural graph tables

        CREATE TABLE IF NOT EXISTS files (
            id INTEGER PRIMARY KEY,
            path TEXT NOT NULL UNIQUE,
            hash TEXT NOT NULL,
            language TEXT,
            indexed_at TEXT NOT NULL DEFAULT (datetime('now'))
        );

        CREATE TABLE IF NOT EXISTS symbols (
            id INTEGER PRIMARY KEY,
            file_id INTEGER NOT NULL REFERENCES files(id),
            name TEXT NOT NULL,
            kind TEXT NOT NULL,
            start_line INTEGER NOT NULL,
            end_line INTEGER NOT NULL,
            parent_symbol_id INTEGER REFERENCES symbols(id),
            signature TEXT
        );

        CREATE TABLE IF NOT EXISTS imports (
            id INTEGER PRIMARY KEY,
            file_id INTEGER NOT NULL REFERENCES files(id),
            source_path TEXT NOT NULL,
            imported_names TEXT,
            kind TEXT NOT NULL DEFAULT 'direct'
        );

        CREATE TABLE IF NOT EXISTS refs (
            id INTEGER PRIMARY KEY,
            file_id INTEGER NOT NULL REFERENCES files(id),
            symbol_name TEXT NOT NULL,
            line INTEGER NOT NULL,
            target_symbol_id INTEGER REFERENCES symbols(id)
        );

        CREATE TABLE IF NOT EXISTS cochange (
            file_id_a INTEGER NOT NULL REFERENCES files(id),
            file_id_b INTEGER NOT NULL REFERENCES files(id),
            correlation REAL NOT NULL,
            commit_count INTEGER NOT NULL,
            PRIMARY KEY (file_id_a, file_id_b)
        );

        -- Summaries table (joined to vec0 via summary_id)

        CREATE TABLE IF NOT EXISTS summaries (
            id INTEGER PRIMARY KEY,
            symbol_id INTEGER REFERENCES symbols(id),
            file_id INTEGER REFERENCES files(id),
            level TEXT NOT NULL,
            summary TEXT NOT NULL,
            token_count INTEGER,
            generated_at TEXT NOT NULL DEFAULT (datetime('now'))
        );

        -- Indexes for structural queries

        CREATE INDEX IF NOT EXISTS idx_imports_file ON imports(file_id);
        CREATE INDEX IF NOT EXISTS idx_imports_source ON imports(source_path);
        CREATE INDEX IF NOT EXISTS idx_symbols_file ON symbols(file_id);
        CREATE INDEX IF NOT EXISTS idx_symbols_name ON symbols(name);
        CREATE INDEX IF NOT EXISTS idx_refs_file ON refs(file_id);
        CREATE INDEX IF NOT EXISTS idx_refs_symbol ON refs(symbol_name);
        CREATE INDEX IF NOT EXISTS idx_cochange_a ON cochange(file_id_a);
        CREATE INDEX IF NOT EXISTS idx_cochange_b ON cochange(file_id_b);
    ",
    )?;

    Ok(())
}

pub fn create_vec_tables(conn: &Connection, dim: usize) -> Result<()> {
    conn.execute_batch(&format!(
        "CREATE VIRTUAL TABLE IF NOT EXISTS summary_embeddings USING vec0(
            summary_id INTEGER PRIMARY KEY,
            embedding float[{dim}]
        );

        CREATE VIRTUAL TABLE IF NOT EXISTS finding_embeddings USING vec0(
            finding_id INTEGER PRIMARY KEY,
            embedding float[{dim}]
        );"
    ))?;

    Ok(())
}
