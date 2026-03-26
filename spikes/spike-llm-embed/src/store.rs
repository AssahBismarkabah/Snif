use anyhow::Result;
use rusqlite::{ffi::sqlite3_auto_extension, Connection};
use sqlite_vec::sqlite3_vec_init;

pub fn init_sqlite_vec() {
    unsafe {
        sqlite3_auto_extension(Some(std::mem::transmute(sqlite3_vec_init as *const ())));
    }
}

pub fn create_db(db_path: &str) -> Result<Connection> {
    let _ = std::fs::remove_file(db_path);

    let conn = Connection::open(db_path)?;
    conn.execute_batch(
        "PRAGMA journal_mode=WAL;
         PRAGMA synchronous=NORMAL;

         CREATE TABLE code_summaries (
             id INTEGER PRIMARY KEY,
             name TEXT NOT NULL,
             file_path TEXT NOT NULL,
             body TEXT NOT NULL,
             summary TEXT,
             input_chars INTEGER,
             output_chars INTEGER,
             summary_time_ms INTEGER
         );

         CREATE VIRTUAL TABLE summary_embeddings USING vec0(
             summary_id INTEGER PRIMARY KEY,
             embedding float[384]
         );",
    )?;

    Ok(conn)
}
