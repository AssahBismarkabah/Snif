use crate::Store;
use anyhow::Result;
use snif_config::constants::retrieval;
use snif_types::{Symbol, SymbolForSummary};

impl Store {
    /// Fetch a single page of symbols for summarization, ordered by kind, file, and start line.
    fn query_symbols_page(&self, offset: i64, limit: i64) -> Result<Vec<SymbolForSummary>> {
        let mut stmt = self.conn.prepare(
            "SELECT s.id, s.name, s.kind, s.start_line, s.end_line, s.file_id, f.path
             FROM symbols s
             JOIN files f ON s.file_id = f.id
             ORDER BY s.kind, s.file_id, s.start_line
             LIMIT ?1 OFFSET ?2",
        )?;
        let rows = stmt
            .query_map(rusqlite::params![limit, offset], |row| {
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

    /// Fetch all symbols for summarization, paginating internally to avoid
    /// hard limits on large repositories. Returns every symbol regardless of
    /// how many exist, fetching in pages of SYMBOLS_PAGE_SIZE.
    pub fn get_symbols_for_summarization(&self) -> Result<Vec<SymbolForSummary>> {
        let page_size = retrieval::SYMBOLS_PAGE_SIZE as i64;
        let mut all_symbols = Vec::new();
        let mut offset = 0;
        loop {
            let page = self.query_symbols_page(offset, page_size)?;
            let count = page.len();
            all_symbols.extend(page);
            if (count as i64) < page_size {
                break;
            }
            offset += page_size;
        }
        Ok(all_symbols)
    }

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

    /// Fetch symbols for specific file IDs. Used by on-demand summarization
    /// to limit work to only the files that need summaries.
    pub fn get_symbols_for_files(&self, file_ids: &[i64]) -> Result<Vec<SymbolForSummary>> {
        if file_ids.is_empty() {
            return Ok(vec![]);
        }
        let placeholders: String = file_ids.iter().map(|_| "?").collect::<Vec<_>>().join(",");
        let sql = format!(
            "SELECT s.id, s.name, s.kind, s.start_line, s.end_line, s.file_id, f.path
             FROM symbols s
             JOIN files f ON s.file_id = f.id
             WHERE s.file_id IN ({})
             ORDER BY s.kind, s.file_id, s.start_line",
            placeholders
        );
        let mut stmt = self.conn.prepare(&sql)?;
        let params: Vec<&dyn rusqlite::types::ToSql> = file_ids
            .iter()
            .map(|id| id as &dyn rusqlite::types::ToSql)
            .collect();
        let rows = stmt
            .query_map(params.as_slice(), |row| {
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
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup_store_with_symbols() -> Store {
        let store = Store::open_in_memory().expect("store should open");
        let file_id = store
            .upsert_file("src/lib.rs", "hash123", "rust")
            .expect("file should insert");

        let conn = store.conn();
        for i in 0..15 {
            conn.execute(
                "INSERT INTO symbols (file_id, name, kind, start_line, end_line) VALUES (?1, ?2, ?3, ?4, ?5)",
                rusqlite::params![file_id, format!("sym_{i}"), "function", i * 10 + 1, i * 10 + 5],
            )
            .expect("symbol should insert");
        }
        store
    }

    #[test]
    fn get_symbols_for_summarization_returns_all_symbols() {
        let store = setup_store_with_symbols();
        let symbols = store
            .get_symbols_for_summarization()
            .expect("should fetch symbols");
        assert_eq!(symbols.len(), 15, "should return all 15 symbols");
    }

    #[test]
    fn query_symbols_page_returns_correct_page() {
        let store = setup_store_with_symbols();

        // Page size of 5 should return first 5 symbols
        let page1 = store.query_symbols_page(0, 5).expect("page 1 should work");
        assert_eq!(page1.len(), 5, "first page should have 5 symbols");

        // Second page
        let page2 = store.query_symbols_page(5, 5).expect("page 2 should work");
        assert_eq!(page2.len(), 5, "second page should have 5 symbols");

        // Third page
        let page3 = store.query_symbols_page(10, 5).expect("page 3 should work");
        assert_eq!(page3.len(), 5, "third page should have 5 symbols");

        // Fourth page should be empty
        let page4 = store.query_symbols_page(15, 5).expect("page 4 should work");
        assert!(page4.is_empty(), "fourth page should be empty");
    }

    #[test]
    fn query_symbols_page_respects_offset() {
        let store = setup_store_with_symbols();

        let page1 = store.query_symbols_page(0, 10).expect("page 1 should work");
        let page2 = store
            .query_symbols_page(10, 10)
            .expect("page 2 should work");

        // No overlap between pages
        let page1_ids: std::collections::HashSet<i64> = page1.iter().map(|s| s.id).collect();
        let page2_ids: std::collections::HashSet<i64> = page2.iter().map(|s| s.id).collect();
        assert!(
            page1_ids.is_disjoint(&page2_ids),
            "pages should not overlap"
        );
    }

    #[test]
    fn pagination_loops_until_all_symbols_returned() {
        let store = setup_store_with_symbols();

        // With a page size smaller than total symbols, get_symbols_for_summarization
        // should still return all of them
        let all_symbols = store
            .get_symbols_for_summarization()
            .expect("should fetch all symbols");
        assert_eq!(
            all_symbols.len(),
            15,
            "should return all symbols regardless of page size"
        );
    }

    #[test]
    fn empty_store_returns_no_symbols() {
        let store = Store::open_in_memory().expect("store should open");
        let symbols = store
            .get_symbols_for_summarization()
            .expect("should succeed on empty store");
        assert!(symbols.is_empty(), "empty store should return no symbols");
    }
}
