use crate::Store;
use anyhow::Result;

impl Store {
    pub fn delete_all_cochange(&self) -> Result<()> {
        self.conn.execute("DELETE FROM cochange", [])?;
        Ok(())
    }

    pub fn insert_cochange_batch(&self, pairs: &[(i64, i64, f64, usize)]) -> Result<()> {
        let tx = self.conn.unchecked_transaction()?;
        {
            let mut stmt = tx.prepare(
                "INSERT OR REPLACE INTO cochange (file_id_a, file_id_b, correlation, commit_count)
                 VALUES (?1, ?2, ?3, ?4)",
            )?;
            for (a, b, corr, count) in pairs {
                stmt.execute(rusqlite::params![a, b, corr, *count as i64])?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    pub fn get_cochange_for_file(
        &self,
        file_id: i64,
        min_correlation: f64,
    ) -> Result<Vec<(i64, f64, i64)>> {
        let mut stmt = self.conn.prepare(
            "SELECT file_id_b, correlation, commit_count FROM cochange
             WHERE file_id_a = ?1 AND correlation >= ?2
             UNION
             SELECT file_id_a, correlation, commit_count FROM cochange
             WHERE file_id_b = ?1 AND correlation >= ?2
             ORDER BY correlation DESC",
        )?;
        let rows = stmt
            .query_map(rusqlite::params![file_id, min_correlation], |row| {
                Ok((row.get(0)?, row.get(1)?, row.get(2)?))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }
}
