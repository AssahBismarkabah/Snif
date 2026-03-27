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
        // Two-step: KNN first, then join in application code
        let knn_results = self.query_similar_summaries(query_embedding, k)?;

        let mut results = Vec::with_capacity(knn_results.len());
        let mut detail_stmt = self
            .conn
            .prepare("SELECT id, file_id, symbol_id, summary FROM summaries WHERE id = ?1")?;

        for (summary_id, distance) in knn_results {
            if let Ok(row) = detail_stmt.query_row([summary_id], |row| {
                Ok(SimilarSummary {
                    summary_id: row.get(0)?,
                    file_id: row.get(1)?,
                    symbol_id: row.get(2)?,
                    summary_text: row.get(3)?,
                    distance,
                })
            }) {
                results.push(row);
            }
        }

        Ok(results)
    }
}
