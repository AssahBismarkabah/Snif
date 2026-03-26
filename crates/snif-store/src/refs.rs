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

    pub fn get_files_referencing_symbols(&self, names: &[String]) -> Result<Vec<(i64, String)>> {
        if names.is_empty() {
            return Ok(vec![]);
        }
        let placeholders: String = names.iter().map(|_| "?").collect::<Vec<_>>().join(",");
        let sql = format!(
            "SELECT DISTINCT r.file_id, r.symbol_name FROM refs r WHERE r.symbol_name IN ({})",
            placeholders
        );
        let mut stmt = self.conn.prepare(&sql)?;
        let params: Vec<&dyn rusqlite::types::ToSql> =
            names.iter().map(|n| n as &dyn rusqlite::types::ToSql).collect();
        let rows = stmt
            .query_map(params.as_slice(), |row| Ok((row.get(0)?, row.get(1)?)))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }
}
