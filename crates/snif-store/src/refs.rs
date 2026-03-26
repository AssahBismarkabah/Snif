use crate::Store;
use anyhow::Result;
use snif_types::Reference;

impl Store {
    pub fn insert_refs(&self, file_id: i64, references: &[Reference]) -> Result<()> {
        let mut stmt = self.conn.prepare(
            "INSERT INTO refs (file_id, symbol_name, line) VALUES (?1, ?2, ?3)",
        )?;

        for r in references {
            stmt.execute(rusqlite::params![file_id, r.name, r.line])?;
        }

        Ok(())
    }

    pub fn delete_refs_for_file(&self, file_id: i64) -> Result<()> {
        self.conn.execute("DELETE FROM refs WHERE file_id = ?1", [file_id])?;
        Ok(())
    }
}
