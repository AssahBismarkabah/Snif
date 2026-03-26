use crate::Store;
use anyhow::Result;

impl Store {
    pub fn upsert_file(&self, path: &str, hash: &str, language: &str) -> Result<i64> {
        self.conn.execute(
            "INSERT INTO files (path, hash, language) VALUES (?1, ?2, ?3)
             ON CONFLICT(path) DO UPDATE SET hash = ?2, language = ?3, indexed_at = datetime('now')",
            rusqlite::params![path, hash, language],
        )?;

        let id: i64 = self.conn.query_row(
            "SELECT id FROM files WHERE path = ?1",
            [path],
            |row| row.get(0),
        )?;

        Ok(id)
    }

    pub fn get_file_hash(&self, path: &str) -> Result<Option<String>> {
        let result = self.conn.query_row(
            "SELECT hash FROM files WHERE path = ?1",
            [path],
            |row| row.get(0),
        );

        match result {
            Ok(hash) => Ok(Some(hash)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }
}
