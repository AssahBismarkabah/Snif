use anyhow::Result;
use rusqlite::Connection;

/// Increment this whenever the schema changes. On open, if the stored version
/// doesn't match, the database is dropped and recreated automatically.
const SCHEMA_VERSION: i64 = 2;

pub fn run_migrations(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS schema_version (
            version INTEGER NOT NULL
        );

        CREATE TABLE IF NOT EXISTS files (
            id INTEGER PRIMARY KEY,
            path TEXT NOT NULL UNIQUE,
            hash TEXT NOT NULL,
            language TEXT,
            indexed_at TEXT NOT NULL DEFAULT (datetime('now'))
        );

        CREATE TABLE IF NOT EXISTS symbols (
            id INTEGER PRIMARY KEY,
            file_id INTEGER NOT NULL REFERENCES files(id) ON DELETE CASCADE,
            name TEXT NOT NULL,
            kind TEXT NOT NULL,
            start_line INTEGER NOT NULL,
            end_line INTEGER NOT NULL,
            parent_symbol_id INTEGER REFERENCES symbols(id),
            signature TEXT
        );

        CREATE TABLE IF NOT EXISTS imports (
            id INTEGER PRIMARY KEY,
            file_id INTEGER NOT NULL REFERENCES files(id) ON DELETE CASCADE,
            source_path TEXT NOT NULL,
            imported_names TEXT,
            kind TEXT NOT NULL DEFAULT 'direct'
        );

        CREATE TABLE IF NOT EXISTS refs (
            id INTEGER PRIMARY KEY,
            file_id INTEGER NOT NULL REFERENCES files(id) ON DELETE CASCADE,
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

        CREATE TABLE IF NOT EXISTS summaries (
            id INTEGER PRIMARY KEY,
            symbol_id INTEGER REFERENCES symbols(id),
            file_id INTEGER REFERENCES files(id),
            level TEXT NOT NULL,
            summary TEXT NOT NULL,
            token_count INTEGER,
            generated_at TEXT NOT NULL DEFAULT (datetime('now'))
        );

        CREATE INDEX IF NOT EXISTS idx_symbols_file ON symbols(file_id);
        CREATE INDEX IF NOT EXISTS idx_symbols_name ON symbols(name);
        CREATE INDEX IF NOT EXISTS idx_imports_file ON imports(file_id);
        CREATE INDEX IF NOT EXISTS idx_imports_source ON imports(source_path);
        CREATE INDEX IF NOT EXISTS idx_refs_file ON refs(file_id);
        CREATE INDEX IF NOT EXISTS idx_refs_symbol ON refs(symbol_name);
        CREATE INDEX IF NOT EXISTS idx_cochange_a ON cochange(file_id_a);
        CREATE INDEX IF NOT EXISTS idx_cochange_b ON cochange(file_id_b);
    ",
    )?;

    // Vec tables created separately — sqlite-vec virtual tables use different syntax
    conn.execute_batch(
        "
        CREATE VIRTUAL TABLE IF NOT EXISTS summary_embeddings USING vec0(
            summary_id INTEGER PRIMARY KEY,
            embedding float[384]
        );

        CREATE VIRTUAL TABLE IF NOT EXISTS finding_embeddings USING vec0(
            finding_id INTEGER PRIMARY KEY,
            embedding float[384]
        );
    ",
    )?;

    // Write current schema version
    conn.execute("DELETE FROM schema_version", [])?;
    conn.execute(
        "INSERT INTO schema_version (version) VALUES (?1)",
        [SCHEMA_VERSION],
    )?;

    Ok(())
}

/// Check if the stored schema version matches the expected version.
/// Returns true if the schema is compatible, false if it needs a reset.
pub fn check_version(conn: &Connection) -> bool {
    let version: Result<i64, _> =
        conn.query_row("SELECT version FROM schema_version LIMIT 1", [], |row| {
            row.get(0)
        });
    match version {
        Ok(v) => v == SCHEMA_VERSION,
        Err(_) => false,
    }
}

pub fn drop_all(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "
        DROP TABLE IF EXISTS finding_embeddings;
        DROP TABLE IF EXISTS summary_embeddings;
        DROP TABLE IF EXISTS summaries;
        DROP TABLE IF EXISTS cochange;
        DROP TABLE IF EXISTS refs;
        DROP TABLE IF EXISTS imports;
        DROP TABLE IF EXISTS symbols;
        DROP TABLE IF EXISTS files;
        DROP TABLE IF EXISTS schema_version;
    ",
    )?;
    Ok(())
}
