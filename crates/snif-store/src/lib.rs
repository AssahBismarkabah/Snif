pub mod cochange;
pub mod embeddings;
pub mod files;
pub mod imports;
pub mod refs;
mod schema;
pub mod sql;
pub mod summaries;
pub mod symbols;

use anyhow::Result;
use rusqlite::{ffi::sqlite3_auto_extension, Connection};
use sqlite_vec::sqlite3_vec_init;
use std::path::Path;
use std::sync::Once;

static INIT_VEC: Once = Once::new();

/// Initialise the sqlite-vec extension for the current process.
///
/// Safe to call from multiple threads — the underlying `Once` guarantees
/// the extension is registered exactly once.
pub fn init_sqlite_vec() {
    INIT_VEC.call_once(|| unsafe {
        #[allow(clippy::missing_transmute_annotations)]
        sqlite3_auto_extension(Some(std::mem::transmute(sqlite3_vec_init as *const ())));
    });
}

pub struct Store {
    conn: Connection,
}

impl Store {
    pub fn open(path: &Path) -> Result<Self> {
        init_sqlite_vec();

        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let conn = Connection::open(path)?;
        // foreign_keys must be OFF: the bundled SQLite is compiled with
        // SQLITE_DEFAULT_FOREIGN_KEYS=1, which enables FK enforcement by
        // default. The schema and delete logic assume standard SQLite
        // behaviour (FK constraints OFF).
        conn.execute_batch(
            "PRAGMA journal_mode=WAL; PRAGMA synchronous=NORMAL; PRAGMA foreign_keys=OFF;",
        )?;

        if !schema::check_version(&conn) {
            tracing::warn!("Schema version mismatch — rebuilding index database");
            schema::drop_all(&conn)?;
        }
        schema::run_migrations(&conn)?;

        tracing::info!(path = %path.display(), "Store opened");
        Ok(Self { conn })
    }

    pub fn open_in_memory() -> Result<Self> {
        init_sqlite_vec();

        let conn = Connection::open_in_memory()?;
        schema::run_migrations(&conn)?;

        Ok(Self { conn })
    }

    pub fn conn(&self) -> &Connection {
        &self.conn
    }

    pub fn reset_schema(&self) -> Result<()> {
        schema::drop_all(&self.conn)?;
        schema::run_migrations(&self.conn)?;
        tracing::info!("Schema reset");
        Ok(())
    }
}
