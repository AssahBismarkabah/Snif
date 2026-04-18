use crate::Store;
use anyhow::Result;

impl Store {
    pub fn insert_summary(
        &self,
        symbol_id: Option<i64>,
        file_id: Option<i64>,
        level: &str,
        summary: &str,
        token_count: Option<i32>,
    ) -> Result<i64> {
        self.conn.execute(
            "INSERT INTO summaries (symbol_id, file_id, level, summary, token_count)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            rusqlite::params![symbol_id, file_id, level, summary, token_count],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    pub fn get_summary_for_symbol(&self, symbol_id: i64) -> Result<Option<(i64, String)>> {
        match self.conn.query_row(
            "SELECT id, summary FROM summaries WHERE symbol_id = ?1",
            [symbol_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        ) {
            Ok(r) => Ok(Some(r)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    pub fn get_summary_for_file(&self, file_id: i64) -> Result<Option<(i64, String)>> {
        match self.conn.query_row(
            "SELECT id, summary FROM summaries WHERE file_id = ?1 AND level = 'file'",
            [file_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        ) {
            Ok(r) => Ok(Some(r)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    pub fn get_summaries_for_file_symbols(
        &self,
        file_id: i64,
    ) -> Result<Vec<(i64, String, String, String)>> {
        let mut stmt = self.conn.prepare(
            "SELECT su.id, s.name, su.level, su.summary
             FROM summaries su
             JOIN symbols s ON su.symbol_id = s.id
             WHERE s.file_id = ?1
             ORDER BY s.start_line",
        )?;
        let rows = stmt
            .query_map([file_id], |row| {
                Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }

    pub fn get_all_summaries(&self) -> Result<Vec<(i64, String)>> {
        let mut stmt = self
            .conn
            .prepare("SELECT id, summary FROM summaries LIMIT 50000")?;
        let rows = stmt
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }

    pub fn delete_summaries_for_file(&self, file_id: i64) -> Result<()> {
        self.conn.execute(
            "DELETE FROM summaries WHERE file_id = ?1 OR symbol_id IN
             (SELECT id FROM symbols WHERE file_id = ?1)",
            [file_id],
        )?;
        Ok(())
    }
}
