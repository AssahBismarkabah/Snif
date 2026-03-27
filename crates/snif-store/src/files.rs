use crate::Store;
use anyhow::Result;

impl Store {
    pub fn upsert_file(&self, path: &str, hash: &str, language: &str) -> Result<i64> {
        self.conn.execute(
            "INSERT INTO files (path, hash, language) VALUES (?1, ?2, ?3)
             ON CONFLICT(path) DO UPDATE SET hash = ?2, language = ?3, indexed_at = datetime('now')",
            rusqlite::params![path, hash, language],
        )?;

        let id: i64 =
            self.conn
                .query_row("SELECT id FROM files WHERE path = ?1", [path], |row| {
                    row.get(0)
                })?;

        Ok(id)
    }

    pub fn get_file_hash(&self, path: &str) -> Result<Option<String>> {
        match self
            .conn
            .query_row("SELECT hash FROM files WHERE path = ?1", [path], |row| {
                row.get(0)
            }) {
            Ok(hash) => Ok(Some(hash)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    pub fn get_file_id(&self, path: &str) -> Result<Option<i64>> {
        match self
            .conn
            .query_row("SELECT id FROM files WHERE path = ?1", [path], |row| {
                row.get(0)
            }) {
            Ok(id) => Ok(Some(id)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    pub fn get_all_file_paths(&self) -> Result<Vec<(i64, String)>> {
        let mut stmt = self.conn.prepare("SELECT id, path FROM files")?;
        let rows = stmt
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }
}
