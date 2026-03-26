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
}
