use crate::Store;
use anyhow::Result;
use snif_types::Import;

impl Store {
    pub fn insert_imports(&self, file_id: i64, imports: &[Import]) -> Result<()> {
        let mut stmt = self.conn.prepare(
            "INSERT INTO imports (file_id, source_path, imported_names, kind)
             VALUES (?1, ?2, ?3, ?4)",
        )?;

        for imp in imports {
            let names = if imp.names.is_empty() {
                None
            } else {
                Some(serde_json::to_string(&imp.names)?)
            };
            stmt.execute(rusqlite::params![file_id, imp.source, names, "direct"])?;
        }

        Ok(())
    }

    pub fn delete_imports_for_file(&self, file_id: i64) -> Result<()> {
        self.conn.execute("DELETE FROM imports WHERE file_id = ?1", [file_id])?;
        Ok(())
    }

    pub fn get_imports_for_file(&self, file_id: i64) -> Result<Vec<(i64, String)>> {
        let mut stmt = self.conn.prepare(
            "SELECT f.id, i.source_path FROM imports i
             JOIN files f ON f.path = i.source_path
             WHERE i.file_id = ?1",
        )?;
        let rows = stmt
            .query_map([file_id], |row| Ok((row.get(0)?, row.get(1)?)))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }

    pub fn get_reverse_imports(&self, file_path: &str) -> Result<Vec<i64>> {
        let mut stmt = self.conn.prepare(
            "SELECT DISTINCT file_id FROM imports WHERE source_path = ?1",
        )?;
        let rows = stmt
            .query_map([file_path], |row| row.get(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }
}
