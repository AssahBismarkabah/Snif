use crate::Store;
use anyhow::Result;
use snif_types::Symbol;

impl Store {
    pub fn insert_symbols(&self, file_id: i64, symbols: &[Symbol]) -> Result<()> {
        let mut stmt = self.conn.prepare(
            "INSERT INTO symbols (file_id, name, kind, start_line, end_line, signature)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        )?;

        for sym in symbols {
            stmt.execute(rusqlite::params![
                file_id,
                sym.name,
                sym.kind.to_string(),
                sym.start_line,
                sym.end_line,
                sym.signature,
            ])?;
        }

        Ok(())
    }

    pub fn delete_symbols_for_file(&self, file_id: i64) -> Result<()> {
        self.conn.execute("DELETE FROM symbols WHERE file_id = ?1", [file_id])?;
        Ok(())
    }
}
