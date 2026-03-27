use crate::Store;
use anyhow::Result;
use snif_types::Symbol;

pub struct SymbolForSummary {
    pub id: i64,
    pub name: String,
    pub kind: String,
    pub start_line: i64,
    pub end_line: i64,
    pub file_id: i64,
    pub file_path: String,
}

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
        self.conn
            .execute("DELETE FROM symbols WHERE file_id = ?1", [file_id])?;
        Ok(())
    }

    pub fn get_symbols_for_summarization(&self) -> Result<Vec<SymbolForSummary>> {
        let mut stmt = self.conn.prepare(
            "SELECT s.id, s.name, s.kind, s.start_line, s.end_line, s.file_id, f.path
             FROM symbols s
             JOIN files f ON s.file_id = f.id
             ORDER BY s.kind, s.file_id, s.start_line",
        )?;
        let rows = stmt
            .query_map([], |row| {
                Ok(SymbolForSummary {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    kind: row.get(2)?,
                    start_line: row.get(3)?,
                    end_line: row.get(4)?,
                    file_id: row.get(5)?,
                    file_path: row.get(6)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }

    pub fn get_files_defining_symbols(&self, names: &[String]) -> Result<Vec<(i64, String)>> {
        if names.is_empty() {
            return Ok(vec![]);
        }
        let placeholders: String = names.iter().map(|_| "?").collect::<Vec<_>>().join(",");
        let sql = format!(
            "SELECT DISTINCT s.file_id, s.name FROM symbols s WHERE s.name IN ({})",
            placeholders
        );
        let mut stmt = self.conn.prepare(&sql)?;
        let params: Vec<&dyn rusqlite::types::ToSql> = names
            .iter()
            .map(|n| n as &dyn rusqlite::types::ToSql)
            .collect();
        let rows = stmt
            .query_map(params.as_slice(), |row| Ok((row.get(0)?, row.get(1)?)))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }
}
