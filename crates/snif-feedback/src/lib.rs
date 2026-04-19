pub mod collector;
pub mod filter;

use anyhow::Result;
use rusqlite::Connection;
use snif_config::constants::model;
use snif_store::{init_sqlite_vec, sql::pragmas};
use std::path::Path;
use zerocopy::AsBytes;

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
        conn.execute_batch(&format!(
            "{}; {}; {};",
            pragmas::JOURNAL_MODE_WAL,
            pragmas::SYNCHRONOUS_NORMAL,
            pragmas::FOREIGN_KEYS_OFF
        ))?;

        conn.execute_batch(&format!(
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
                embedding float[{}]
            );",
            model::DEFAULT_EMBEDDING_DIMENSION
        ))?;

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
        use std::collections::HashMap;

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

        if knn_results.is_empty() {
            return Ok(Vec::new());
        }

        // Batch fetch with IN clause instead of N+1
        let signal_ids: Vec<i64> = knn_results.iter().map(|(id, _)| *id).collect();
        let distance_map: HashMap<i64, f64> = knn_results.into_iter().collect();

        // SQLite has a default limit of 999 variables per query.
        // Chunk to stay well under that limit.
        const SQLITE_MAX_VARIABLE_NUMBER: usize = 900;

        let mut results = Vec::new();
        for chunk in signal_ids.chunks(SQLITE_MAX_VARIABLE_NUMBER) {
            let placeholders: String = chunk.iter().map(|_| "?").collect::<Vec<_>>().join(",");
            let sql = format!(
                "SELECT id, signal_type FROM feedback_signals 
                 WHERE id IN ({}) AND team_id = ?",
                placeholders
            );

            let mut stmt = self.conn.prepare(&sql)?;
            let mut params: Vec<&dyn rusqlite::ToSql> = chunk
                .iter()
                .map(|id| id as &dyn rusqlite::ToSql)
                .collect();
            params.push(&team_id);

            let chunk_results = stmt
                .query_map(params.as_slice(), |row| {
                    let id: i64 = row.get(0)?;
                    let signal_type: String = row.get(1)?;
                    let distance = *distance_map.get(&id).unwrap_or(&0.0);
                    Ok((signal_type, distance))
                })?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            results.extend(chunk_results);
        }

        Ok(results)
    }
}
