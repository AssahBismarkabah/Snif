pub mod collector;
pub mod filter;

use anyhow::Result;
use rusqlite::{ffi::sqlite3_auto_extension, Connection};
use sqlite_vec::sqlite3_vec_init;
use std::path::Path;
use std::sync::Once;
use zerocopy::AsBytes;

static INIT_VEC: Once = Once::new();

fn init_sqlite_vec() {
    INIT_VEC.call_once(|| unsafe {
        #[allow(clippy::missing_transmute_annotations)]
        sqlite3_auto_extension(Some(std::mem::transmute(sqlite3_vec_init as *const ())));
    });
}

pub struct FeedbackStore {
    conn: Connection,
}

impl FeedbackStore {
    pub fn open(path: &Path) -> Result<Self> {
        init_sqlite_vec();

        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let conn = Connection::open(path)?;
        conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA synchronous=NORMAL;")?;

        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS feedback_signals (
                id INTEGER PRIMARY KEY,
                team_id TEXT NOT NULL,
                signal_type TEXT NOT NULL,
                finding_text TEXT NOT NULL,
                finding_category TEXT NOT NULL,
                timestamp TEXT NOT NULL DEFAULT (datetime('now'))
            );

            CREATE INDEX IF NOT EXISTS idx_signals_team ON feedback_signals(team_id);

            CREATE VIRTUAL TABLE IF NOT EXISTS feedback_embeddings USING vec0(
                signal_id INTEGER PRIMARY KEY,
                embedding float[384]
            );",
        )?;

        Ok(Self { conn })
    }

    pub fn insert_signal(
        &self,
        team_id: &str,
        signal_type: &str,
        finding_text: &str,
        finding_category: &str,
    ) -> Result<i64> {
        self.conn.execute(
            "INSERT INTO feedback_signals (team_id, signal_type, finding_text, finding_category)
             VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params![team_id, signal_type, finding_text, finding_category],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    pub fn insert_signal_embedding(&self, signal_id: i64, embedding: &[f32]) -> Result<()> {
        self.conn.execute(
            "INSERT INTO feedback_embeddings (signal_id, embedding) VALUES (?1, ?2)",
            rusqlite::params![signal_id, embedding.as_bytes()],
        )?;
        Ok(())
    }

    pub fn get_signal_count(&self, team_id: &str) -> Result<usize> {
        let count: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM feedback_signals WHERE team_id = ?1",
            [team_id],
            |row| row.get(0),
        )?;
        Ok(count as usize)
    }

    pub fn query_similar_signals(
        &self,
        query_embedding: &[f32],
        team_id: &str,
        k: usize,
    ) -> Result<Vec<(String, f64)>> {
        // KNN search against all embeddings
        let mut knn_stmt = self.conn.prepare(
            "SELECT signal_id, distance FROM feedback_embeddings
             WHERE embedding MATCH ?1 AND k = ?2
             ORDER BY distance",
        )?;

        let knn_results: Vec<(i64, f64)> = knn_stmt
            .query_map(
                rusqlite::params![query_embedding.as_bytes(), k as i64],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )?
            .collect::<rusqlite::Result<Vec<_>>>()?;

        // Filter by team_id and get signal_type in application code
        let mut results = Vec::new();
        let mut detail_stmt = self
            .conn
            .prepare("SELECT signal_type FROM feedback_signals WHERE id = ?1 AND team_id = ?2")?;

        for (signal_id, distance) in knn_results {
            if let Ok(signal_type) = detail_stmt
                .query_row(rusqlite::params![signal_id, team_id], |row| {
                    row.get::<_, String>(0)
                })
            {
                results.push((signal_type, distance));
            }
        }

        Ok(results)
    }
}
