use crate::Store;
use anyhow::Result;

/// A code chunk representing a segment of source code from a file.
#[derive(Debug, Clone)]
pub struct CodeChunk {
    pub id: i64,
    pub file_id: i64,
    pub start_line: i64,
    pub end_line: i64,
    pub content: String,
    pub content_hash: String,
}

impl Store {
    /// Insert a code chunk and return its row ID.
    pub fn insert_code_chunk(
        &self,
        file_id: i64,
        start_line: i64,
        end_line: i64,
        content: &str,
        content_hash: &str,
    ) -> Result<i64> {
        self.conn.execute(
            "INSERT INTO code_chunks (file_id, start_line, end_line, content, content_hash)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            rusqlite::params![file_id, start_line, end_line, content, content_hash],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    /// Get all code chunks for a specific file.
    pub fn get_chunks_for_file(&self, file_id: i64) -> Result<Vec<CodeChunk>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, file_id, start_line, end_line, content, content_hash
             FROM code_chunks WHERE file_id = ?1
             ORDER BY start_line",
        )?;
        let rows = stmt
            .query_map([file_id], |row| {
                Ok(CodeChunk {
                    id: row.get(0)?,
                    file_id: row.get(1)?,
                    start_line: row.get(2)?,
                    end_line: row.get(3)?,
                    content: row.get(4)?,
                    content_hash: row.get(5)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }

    /// Get all code chunks that do not yet have embeddings.
    /// Returns chunks whose ID is not present in the code_embeddings virtual table.
    pub fn get_unembedded_chunks(&self) -> Result<Vec<CodeChunk>> {
        let mut stmt = self.conn.prepare(
            "SELECT c.id, c.file_id, c.start_line, c.end_line, c.content, c.content_hash
             FROM code_chunks c
             WHERE c.id NOT IN (SELECT chunk_id FROM code_embeddings)
             ORDER BY c.id",
        )?;
        let rows = stmt
            .query_map([], |row| {
                Ok(CodeChunk {
                    id: row.get(0)?,
                    file_id: row.get(1)?,
                    start_line: row.get(2)?,
                    end_line: row.get(3)?,
                    content: row.get(4)?,
                    content_hash: row.get(5)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }

    /// Delete all code chunks for a specific file (for re-indexing).
    /// Also deletes any associated embeddings from the code_embeddings table,
    /// since the vec0 virtual table has no foreign key relationship and
    /// PRAGMA foreign_keys is OFF.
    pub fn delete_chunks_for_file(&self, file_id: i64) -> Result<()> {
        // First collect chunk IDs so we can delete their embeddings
        let chunk_ids: Vec<i64> = {
            let mut stmt = self
                .conn
                .prepare("SELECT id FROM code_chunks WHERE file_id = ?1")?;
            let ids = stmt
                .query_map([file_id], |row| row.get(0))?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            ids
        };

        if !chunk_ids.is_empty() {
            // Delete embeddings for the collected chunk IDs
            let placeholders: String = chunk_ids.iter().map(|_| "?").collect::<Vec<_>>().join(",");
            let sql = format!(
                "DELETE FROM code_embeddings WHERE chunk_id IN ({})",
                placeholders
            );
            let mut stmt = self.conn.prepare(&sql)?;
            let params: Vec<&dyn rusqlite::types::ToSql> = chunk_ids
                .iter()
                .map(|id| id as &dyn rusqlite::types::ToSql)
                .collect();
            stmt.execute(params.as_slice())?;
        }

        // Then delete the chunks themselves
        self.conn
            .execute("DELETE FROM code_chunks WHERE file_id = ?1", [file_id])?;
        Ok(())
    }

    /// Get all code chunk IDs that already have content hashes matching the given hash.
    /// Used for deduplication during re-indexing.
    pub fn get_chunk_ids_by_hash(&self, content_hash: &str) -> Result<Vec<i64>> {
        let mut stmt = self
            .conn
            .prepare("SELECT id FROM code_chunks WHERE content_hash = ?1")?;
        let ids = stmt
            .query_map([content_hash], |row| row.get(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(ids)
    }

    /// Get all file paths and their languages from the files table, used for chunking.
    /// Returns (file_id, path, language) tuples.
    pub fn get_files_for_chunking(&self) -> Result<Vec<(i64, String, Option<String>)>> {
        let mut stmt = self
            .conn
            .prepare("SELECT id, path, language FROM files ORDER BY id")?;
        let rows = stmt
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup_store() -> Store {
        Store::open_in_memory().expect("store should open")
    }

    #[test]
    fn insert_and_retrieve_code_chunk() {
        let store = setup_store();
        let file_id = store
            .upsert_file("src/main.rs", "hash123", "rust")
            .expect("file should insert");

        let chunk_id = store
            .insert_code_chunk(file_id, 1, 50, "fn main() { ... }", "abc123")
            .expect("chunk should insert");

        let chunks = store.get_chunks_for_file(file_id).expect("should query");
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0].id, chunk_id);
        assert_eq!(chunks[0].start_line, 1);
        assert_eq!(chunks[0].end_line, 50);
        assert_eq!(chunks[0].content, "fn main() { ... }");
        assert_eq!(chunks[0].content_hash, "abc123");
    }

    #[test]
    fn get_unembedded_chunks_returns_only_unembedded() {
        let store = setup_store();
        let file_id = store
            .upsert_file("src/lib.rs", "hash456", "rust")
            .expect("file should insert");

        let chunk_id = store
            .insert_code_chunk(file_id, 1, 50, "content1", "hash1")
            .expect("chunk should insert");

        // Before embedding, all chunks are unembedded
        let unembedded = store.get_unembedded_chunks().expect("should query");
        assert_eq!(unembedded.len(), 1);

        // Embed the chunk
        store
            .insert_code_embeddings_batch(&[(
                chunk_id,
                vec![0.0f32; snif_config::constants::model::DEFAULT_EMBEDDING_DIMENSION],
            )])
            .expect("embedding should insert");

        // Now no unembedded chunks
        let unembedded = store.get_unembedded_chunks().expect("should query");
        assert!(unembedded.is_empty());
    }

    #[test]
    fn delete_chunks_for_file_removes_chunks_and_embeddings() {
        let store = setup_store();
        let file_a = store
            .upsert_file("src/a.rs", "hash_a", "rust")
            .expect("file A should insert");
        let file_b = store
            .upsert_file("src/b.rs", "hash_b", "rust")
            .expect("file B should insert");

        let chunk_a = store
            .insert_code_chunk(file_a, 1, 50, "content_a", "hash_a_chunk")
            .expect("chunk A should insert");
        let chunk_b = store
            .insert_code_chunk(file_b, 1, 50, "content_b", "hash_b_chunk")
            .expect("chunk B should insert");

        // Embed both chunks
        store
            .insert_code_embeddings_batch(&[
                (
                    chunk_a,
                    vec![0.0f32; snif_config::constants::model::DEFAULT_EMBEDDING_DIMENSION],
                ),
                (
                    chunk_b,
                    vec![0.0f32; snif_config::constants::model::DEFAULT_EMBEDDING_DIMENSION],
                ),
            ])
            .expect("embeddings should insert");

        // Verify both chunks are embedded
        let embedded = store.get_embedded_chunk_ids().expect("should query");
        assert_eq!(embedded.len(), 2, "both chunks should have embeddings");

        // Delete file A's chunks — this should also delete chunk A's embedding
        store
            .delete_chunks_for_file(file_a)
            .expect("delete should succeed");

        let chunks_a = store.get_chunks_for_file(file_a).expect("should query");
        assert!(chunks_a.is_empty(), "file A chunks should be deleted");

        let chunks_b = store.get_chunks_for_file(file_b).expect("should query");
        assert_eq!(chunks_b.len(), 1, "file B chunks should remain");

        // Verify file A's embedding was deleted but file B's remains
        let embedded_after = store.get_embedded_chunk_ids().expect("should query");
        assert_eq!(
            embedded_after.len(),
            1,
            "only file B's embedding should remain"
        );
        assert!(
            embedded_after.contains(&chunk_b),
            "file B embedding should still exist"
        );
    }

    #[test]
    fn get_chunk_ids_by_hash_returns_matching() {
        let store = setup_store();
        let file_id = store
            .upsert_file("src/app.rs", "hash789", "rust")
            .expect("file should insert");

        store
            .insert_code_chunk(file_id, 1, 50, "content1", "unique_hash_1")
            .expect("chunk should insert");
        store
            .insert_code_chunk(file_id, 51, 100, "content2", "unique_hash_2")
            .expect("chunk should insert");

        let ids = store
            .get_chunk_ids_by_hash("unique_hash_1")
            .expect("should query");
        assert_eq!(ids.len(), 1);

        let ids = store
            .get_chunk_ids_by_hash("nonexistent")
            .expect("should query");
        assert!(ids.is_empty());
    }
}
