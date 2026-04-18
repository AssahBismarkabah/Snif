use crate::Store;
use anyhow::Result;
use zerocopy::AsBytes;

pub struct SimilarSummary {
    pub summary_id: i64,
    pub file_id: Option<i64>,
    pub symbol_id: Option<i64>,
    pub summary_text: String,
    pub distance: f64,
}

impl Store {
    pub fn insert_summary_embeddings_batch(&self, entries: &[(i64, Vec<f32>)]) -> Result<()> {
        let tx = self.conn.unchecked_transaction()?;
        {
            let mut stmt = tx.prepare(
                "INSERT INTO summary_embeddings (summary_id, embedding) VALUES (?1, ?2)",
            )?;
            for (id, embedding) in entries {
                stmt.execute(rusqlite::params![id, embedding.as_bytes()])?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    pub fn get_embedded_summary_ids(&self) -> Result<Vec<i64>> {
        let mut stmt = self
            .conn
            .prepare("SELECT summary_id FROM summary_embeddings")?;
        let ids = stmt
            .query_map([], |row| row.get(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(ids)
    }

    pub fn delete_all_summary_embeddings(&self) -> Result<()> {
        self.conn.execute("DELETE FROM summary_embeddings", [])?;
        Ok(())
    }

    pub fn query_similar_summaries(
        &self,
        query_embedding: &[f32],
        k: usize,
    ) -> Result<Vec<(i64, f64)>> {
        let mut stmt = self.conn.prepare(
            "SELECT summary_id, distance
             FROM summary_embeddings
             WHERE embedding MATCH ?1
               AND k = ?2
             ORDER BY distance",
        )?;
        let rows = stmt
            .query_map(
                rusqlite::params![query_embedding.as_bytes(), k as i64],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }

    pub fn query_similar_summaries_with_details(
        &self,
        query_embedding: &[f32],
        k: usize,
    ) -> Result<Vec<SimilarSummary>> {
        // Single query with JOIN instead of N+1 pattern
        let mut stmt = self.conn.prepare(
            "SELECT s.id, s.file_id, s.symbol_id, s.summary, se.distance
             FROM summary_embeddings se
             JOIN summaries s ON se.summary_id = s.id
             WHERE se.embedding MATCH ?1
               AND se.k = ?2
             ORDER BY se.distance",
        )?;
        let rows = stmt
            .query_map(
                rusqlite::params![query_embedding.as_bytes(), k as i64],
                |row| {
                    Ok(SimilarSummary {
                        summary_id: row.get(0)?,
                        file_id: row.get(1)?,
                        symbol_id: row.get(2)?,
                        summary_text: row.get(3)?,
                        distance: row.get(4)?,
                    })
                },
            )?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }
}
