use crate::Store;
use anyhow::Result;
use snif_config::constants::limits::SQLITE_MAX_VARIABLE_NUMBER;

impl Store {
    pub fn upsert_file(&self, path: &str, hash: &str, language: &str) -> Result<i64> {
        let id: i64 = self.conn.query_row(
            "INSERT INTO files (path, hash, language) VALUES (?1, ?2, ?3)
             ON CONFLICT(path) DO UPDATE SET hash = ?2, language = ?3, indexed_at = datetime('now')
             RETURNING id",
            rusqlite::params![path, hash, language],
            |row| row.get(0),
        )?;
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

    pub fn get_file_ids_batch(&self, paths: &[String]) -> Result<Vec<(i64, String)>> {
        if paths.is_empty() {
            return Ok(Vec::new());
        }

        // SQLite has a default limit of 999 variables per query.
        // Chunk to stay well under that limit using the centralized constant.
        let mut results = Vec::new();
        for chunk in paths.chunks(SQLITE_MAX_VARIABLE_NUMBER) {
            let placeholders: String = chunk.iter().map(|_| "?").collect::<Vec<_>>().join(",");
            let sql = format!(
                "SELECT id, path FROM files WHERE path IN ({})",
                placeholders
            );

            let mut stmt = self.conn.prepare(&sql)?;
            let params: Vec<&dyn rusqlite::ToSql> =
                chunk.iter().map(|p| p as &dyn rusqlite::ToSql).collect();

            let rows = stmt
                .query_map(params.as_slice(), |row| Ok((row.get(0)?, row.get(1)?)))?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            results.extend(rows);
        }
        Ok(results)
    }

    pub fn get_all_file_paths(&self) -> Result<Vec<(i64, String)>> {
        let mut stmt = self.conn.prepare("SELECT id, path FROM files")?;
        let rows = stmt
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }
}
