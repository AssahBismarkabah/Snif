use crate::Store;
use anyhow::Result;
use snif_config::constants::retrieval;

impl Store {
    pub fn insert_summary(
        &self,
        symbol_id: Option<i64>,
        file_id: Option<i64>,
        level: &str,
        summary: &str,
        content_hash: Option<&str>,
        token_count: Option<i32>,
    ) -> Result<i64> {
        self.conn.execute(
            "INSERT INTO summaries (symbol_id, file_id, level, summary, content_hash, token_count)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            rusqlite::params![
                symbol_id,
                file_id,
                level,
                summary,
                content_hash,
                token_count
            ],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    pub fn get_summary_for_symbol(
        &self,
        symbol_id: i64,
    ) -> Result<Option<(i64, String, Option<String>)>> {
        match self.conn.query_row(
            "SELECT id, summary, content_hash FROM summaries WHERE symbol_id = ?1",
            [symbol_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        ) {
            Ok(r) => Ok(Some(r)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    pub fn get_summary_for_file(&self, file_id: i64) -> Result<Option<(i64, String)>> {
        match self.conn.query_row(
            "SELECT id, summary FROM summaries WHERE file_id = ?1 AND level = 'file'",
            [file_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        ) {
            Ok(r) => Ok(Some(r)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    pub fn get_summaries_for_file_symbols(
        &self,
        file_id: i64,
    ) -> Result<Vec<(i64, String, String, String)>> {
        let mut stmt = self.conn.prepare(
            "SELECT su.id, s.name, su.level, su.summary
             FROM summaries su
             JOIN symbols s ON su.symbol_id = s.id
             WHERE s.file_id = ?1
             ORDER BY s.start_line",
        )?;
        let rows = stmt
            .query_map([file_id], |row| {
                Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }

    /// Fetch a single page of summaries for embedding.
    fn query_summaries_page(&self, offset: i64, limit: i64) -> Result<Vec<(i64, String)>> {
        let mut stmt = self
            .conn
            .prepare("SELECT id, summary FROM summaries ORDER BY id LIMIT ?1 OFFSET ?2")?;
        let rows = stmt
            .query_map(rusqlite::params![limit, offset], |row| {
                Ok((row.get(0)?, row.get(1)?))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }

    /// Fetch all summaries, paginating internally to avoid hard limits on large
    /// repositories. Returns every summary regardless of how many exist, fetching
    /// in pages of SUMMARIES_PAGE_SIZE.
    pub fn get_all_summaries(&self) -> Result<Vec<(i64, String)>> {
        let page_size = retrieval::SUMMARIES_PAGE_SIZE as i64;
        let mut all_rows = Vec::new();
        let mut offset = 0;
        loop {
            let page = self.query_summaries_page(offset, page_size)?;
            let count = page.len();
            all_rows.extend(page);
            if (count as i64) < page_size {
                break;
            }
            offset += page_size;
        }
        Ok(all_rows)
    }

    pub fn delete_summaries_for_file(&self, file_id: i64) -> Result<()> {
        self.conn.execute(
            "DELETE FROM summaries WHERE file_id = ?1 OR symbol_id IN
             (SELECT id FROM symbols WHERE file_id = ?1)",
            [file_id],
        )?;
        Ok(())
    }

    /// Delete summaries and their embeddings for a list of file IDs.
    /// Used to clear stale summaries before re-summarization of changed files.
    /// Also deletes associated embeddings from the summary_embeddings virtual table.
    pub fn delete_summaries_for_files(&self, file_ids: &[i64]) -> Result<()> {
        if file_ids.is_empty() {
            return Ok(());
        }

        // Collect summary IDs first so we can delete their embeddings
        let placeholders: String = file_ids.iter().map(|_| "?").collect::<Vec<_>>().join(",");
        let find_sql = format!(
            "SELECT id FROM summaries WHERE file_id IN ({}) OR symbol_id IN
             (SELECT id FROM symbols WHERE file_id IN ({}))",
            placeholders, placeholders
        );

        let mut find_stmt = self.conn.prepare(&find_sql)?;
        let params: Vec<&dyn rusqlite::ToSql> = file_ids
            .iter()
            .chain(file_ids.iter())
            .map(|id| id as &dyn rusqlite::ToSql)
            .collect();
        let summary_ids: Vec<i64> = find_stmt
            .query_map(params.as_slice(), |row| row.get(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?;

        // Delete embeddings for the collected summary IDs
        if !summary_ids.is_empty() {
            let embed_placeholders: String = summary_ids
                .iter()
                .map(|_| "?")
                .collect::<Vec<_>>()
                .join(",");
            let embed_sql = format!(
                "DELETE FROM summary_embeddings WHERE summary_id IN ({})",
                embed_placeholders
            );
            let mut embed_stmt = self.conn.prepare(&embed_sql)?;
            let embed_params: Vec<&dyn rusqlite::ToSql> = summary_ids
                .iter()
                .map(|id| id as &dyn rusqlite::ToSql)
                .collect();
            embed_stmt.execute(embed_params.as_slice())?;
        }

        // Delete the summaries themselves
        let delete_sql = format!(
            "DELETE FROM summaries WHERE file_id IN ({}) OR symbol_id IN
             (SELECT id FROM symbols WHERE file_id IN ({}))",
            placeholders, placeholders
        );
        let mut delete_stmt = self.conn.prepare(&delete_sql)?;
        let delete_params: Vec<&dyn rusqlite::ToSql> = file_ids
            .iter()
            .chain(file_ids.iter())
            .map(|id| id as &dyn rusqlite::ToSql)
            .collect();
        delete_stmt.execute(delete_params.as_slice())?;

        Ok(())
    }

    /// Check whether a symbol's content has changed since its summary was generated.
    /// Returns Some(hash) if the symbol has a summary with a stored content hash,
    /// None if no summary exists or no hash was stored (legacy summaries).
    pub fn get_symbol_content_hash(&self, symbol_id: i64) -> Result<Option<String>> {
        match self.conn.query_row(
            "SELECT content_hash FROM summaries WHERE symbol_id = ?1",
            [symbol_id],
            |row| row.get(0),
        ) {
            Ok(hash) => Ok(hash),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    /// Check whether a file-level summary's source content has changed.
    /// File-level summaries derive from child symbol summaries, so their
    /// content hash is computed from the concatenated symbol hashes.
    pub fn get_file_content_hash(&self, file_id: i64) -> Result<Option<String>> {
        match self.conn.query_row(
            "SELECT content_hash FROM summaries WHERE file_id = ?1 AND level = 'file'",
            [file_id],
            |row| row.get(0),
        ) {
            Ok(hash) => Ok(hash),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use snif_config::constants::summarizer;

    fn setup_store() -> Store {
        Store::open_in_memory().expect("store should open")
    }

    fn insert_file(store: &Store, path: &str) -> i64 {
        store
            .upsert_file(path, "hash123", "rust")
            .expect("file should insert")
    }

    fn insert_symbol_row(
        conn: &rusqlite::Connection,
        file_id: i64,
        name: &str,
        kind: &str,
        start_line: i64,
        end_line: i64,
    ) -> i64 {
        conn.execute(
            "INSERT INTO symbols (file_id, name, kind, start_line, end_line) VALUES (?1, ?2, ?3, ?4, ?5)",
            rusqlite::params![file_id, name, kind, start_line, end_line],
        )
        .expect("symbol should insert");
        conn.last_insert_rowid()
    }

    #[test]
    fn insert_and_retrieve_summary_with_content_hash() {
        let store = setup_store();
        let file_id = insert_file(&store, "src/lib.rs");

        let id = store
            .insert_summary(
                None,
                Some(file_id),
                summarizer::LEVEL_FILE,
                "A utility module for parsing.",
                Some("abc123"),
                Some(8),
            )
            .expect("summary should insert");

        let result = store
            .get_file_content_hash(file_id)
            .expect("query should succeed");
        assert_eq!(result, Some("abc123".to_string()));

        // Verify the summary itself is still retrievable
        let (retrieved_id, text) = store
            .get_summary_for_file(file_id)
            .expect("query should succeed")
            .expect("should have a summary");
        assert_eq!(retrieved_id, id);
        assert_eq!(text, "A utility module for parsing.");
    }

    #[test]
    fn insert_summary_without_content_hash_works() {
        let store = setup_store();
        let file_id = insert_file(&store, "src/main.rs");

        store
            .insert_summary(
                None,
                Some(file_id),
                summarizer::LEVEL_FILE,
                "The main entrypoint.",
                None,
                Some(4),
            )
            .expect("summary should insert");

        let hash = store
            .get_file_content_hash(file_id)
            .expect("query should succeed");
        assert_eq!(hash, None);
    }

    #[test]
    fn symbol_content_hash_round_trips() {
        let store = setup_store();
        let file_id = insert_file(&store, "src/parser.rs");
        let symbol_id = insert_symbol_row(store.conn(), file_id, "parse_line", "function", 10, 25);

        store
            .insert_summary(
                Some(symbol_id),
                None,
                summarizer::KIND_FUNCTION,
                "Parses a single line of input.",
                Some("hash_parse_line_v1"),
                Some(6),
            )
            .expect("summary should insert");

        let retrieved = store
            .get_symbol_content_hash(symbol_id)
            .expect("query should succeed");
        assert_eq!(retrieved, Some("hash_parse_line_v1".to_string()));
    }

    #[test]
    fn get_symbol_content_hash_returns_none_for_missing_symbol() {
        let store = setup_store();

        let result = store
            .get_symbol_content_hash(9999)
            .expect("query should succeed");
        assert_eq!(result, None);
    }

    #[test]
    fn delete_summaries_for_files_removes_targeted_summaries_and_embeddings() {
        let store = setup_store();
        let file_a = insert_file(&store, "src/a.rs");
        let file_b = insert_file(&store, "src/b.rs");

        let id_a = store
            .insert_summary(
                None,
                Some(file_a),
                summarizer::LEVEL_FILE,
                "Module A.",
                Some("hash_a"),
                Some(5),
            )
            .expect("summary A should insert");

        // Insert an embedding for summary A
        store
            .insert_summary_embeddings_batch(&[(
                id_a,
                vec![0.0f32; snif_config::constants::model::DEFAULT_EMBEDDING_DIMENSION],
            )])
            .expect("embedding should insert");

        let id_b = store
            .insert_summary(
                None,
                Some(file_b),
                summarizer::LEVEL_FILE,
                "Module B.",
                Some("hash_b"),
                Some(5),
            )
            .expect("summary B should insert");

        store
            .insert_summary_embeddings_batch(&[(
                id_b,
                vec![0.0f32; snif_config::constants::model::DEFAULT_EMBEDDING_DIMENSION],
            )])
            .expect("embedding B should insert");

        store
            .delete_summaries_for_files(&[file_a])
            .expect("delete should succeed");

        assert!(
            store
                .get_summary_for_file(file_a)
                .expect("query should succeed")
                .is_none(),
            "file A summary should be deleted"
        );
        assert!(
            store
                .get_summary_for_file(file_b)
                .expect("query should succeed")
                .is_some(),
            "file B summary should remain"
        );

        // Verify that summary A's embedding was also deleted
        let embedded_ids = store
            .get_embedded_summary_ids()
            .expect("query should succeed");
        assert!(
            !embedded_ids.contains(&id_a),
            "embedding for deleted summary A should be gone"
        );
        assert!(
            embedded_ids.contains(&id_b),
            "embedding for remaining summary B should still exist"
        );
    }

    #[test]
    fn delete_summaries_for_files_handles_empty_list() {
        let store = setup_store();
        store
            .delete_summaries_for_files(&[])
            .expect("empty delete should succeed");
    }

    #[test]
    fn get_summary_for_symbol_returns_hash() {
        let store = setup_store();
        let file_id = insert_file(&store, "src/utils.rs");
        let symbol_id = insert_symbol_row(store.conn(), file_id, "format_date", "function", 5, 15);

        store
            .insert_summary(
                Some(symbol_id),
                None,
                summarizer::KIND_FUNCTION,
                "Formats a date object into a string.",
                Some("hash_fmt_date_v1"),
                Some(7),
            )
            .expect("summary should insert");

        let result = store
            .get_summary_for_symbol(symbol_id)
            .expect("query should succeed")
            .expect("should have a summary");

        assert_eq!(result.0, symbol_id);
        assert_eq!(result.1, "Formats a date object into a string.");
        assert_eq!(result.2, Some("hash_fmt_date_v1".to_string()));
    }

    #[test]
    fn legacy_summaries_without_hash_still_retrievable() {
        let store = setup_store();
        let file_id = insert_file(&store, "src/legacy.rs");
        let symbol_id = insert_symbol_row(store.conn(), file_id, "old_function", "function", 1, 10);

        // Insert a summary without content_hash (legacy behavior)
        store
            .insert_summary(
                Some(symbol_id),
                None,
                summarizer::KIND_FUNCTION,
                "A legacy summary without hash.",
                None,
                Some(5),
            )
            .expect("legacy summary should insert");

        let result = store
            .get_summary_for_symbol(symbol_id)
            .expect("query should succeed")
            .expect("should have a summary");

        assert_eq!(result.1, "A legacy summary without hash.");
        assert_eq!(result.2, None, "legacy summary should have no content_hash");

        let hash_result = store
            .get_symbol_content_hash(symbol_id)
            .expect("query should succeed");
        assert_eq!(
            hash_result, None,
            "legacy summary content_hash should be None"
        );
    }

    #[test]
    fn get_all_summaries_returns_all_across_pages() {
        let store = setup_store();

        // Insert 15 summaries — more than the test page size would be,
        // but since SUMMARIES_PAGE_SIZE is 50_000, we test that
        // get_all_summaries faithfully returns everything.
        for i in 0..15 {
            let file_id = insert_file(&store, &format!("src/file_{i}.rs"));
            store
                .insert_summary(
                    None,
                    Some(file_id),
                    summarizer::LEVEL_FILE,
                    &format!("Summary for file {i}."),
                    Some(&format!("hash_{i}")),
                    Some(5),
                )
                .expect("summary should insert");
        }

        let all = store
            .get_all_summaries()
            .expect("should fetch all summaries");
        assert_eq!(all.len(), 15, "should return all 15 summaries");
    }

    #[test]
    fn query_summaries_page_returns_subset() {
        let store = setup_store();

        for i in 0..10 {
            let file_id = insert_file(&store, &format!("src/page_{i}.rs"));
            store
                .insert_summary(
                    None,
                    Some(file_id),
                    summarizer::LEVEL_FILE,
                    &format!("Summary {i}."),
                    None,
                    Some(3),
                )
                .expect("summary should insert");
        }

        let page1 = store
            .query_summaries_page(0, 5)
            .expect("page 1 should work");
        assert_eq!(page1.len(), 5, "first page should have 5 summaries");

        let page2 = store
            .query_summaries_page(5, 5)
            .expect("page 2 should work");
        assert_eq!(page2.len(), 5, "second page should have 5 summaries");

        let page3 = store
            .query_summaries_page(10, 5)
            .expect("page 3 should work");
        assert!(page3.is_empty(), "third page should be empty");
    }

    #[test]
    fn query_summaries_pages_do_not_overlap() {
        let store = setup_store();

        for i in 0..10 {
            let file_id = insert_file(&store, &format!("src/overlap_{i}.rs"));
            store
                .insert_summary(
                    None,
                    Some(file_id),
                    summarizer::LEVEL_FILE,
                    &format!("Summary {i}."),
                    None,
                    Some(3),
                )
                .expect("summary should insert");
        }

        let page1 = store.query_summaries_page(0, 5).expect("page 1");
        let page2 = store.query_summaries_page(5, 5).expect("page 2");

        let page1_ids: std::collections::HashSet<i64> = page1.iter().map(|(id, _)| *id).collect();
        let page2_ids: std::collections::HashSet<i64> = page2.iter().map(|(id, _)| *id).collect();

        assert!(
            page1_ids.is_disjoint(&page2_ids),
            "pages should not overlap"
        );
    }

    #[test]
    fn empty_store_returns_no_summaries() {
        let store = Store::open_in_memory().expect("store should open");
        let summaries = store
            .get_all_summaries()
            .expect("should succeed on empty store");
        assert!(
            summaries.is_empty(),
            "empty store should return no summaries"
        );
    }
}
