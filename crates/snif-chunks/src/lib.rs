use anyhow::Result;
use sha2::{Digest, Sha256};
use snif_config::constants::{chunks, limits};
use snif_store::Store;
use std::path::Path;

/// Statistics returned by the chunking operation.
#[derive(Debug)]
pub struct ChunkStats {
    pub chunks_created: usize,
    pub chunks_skipped_unchanged: usize,
    pub files_processed: usize,
    pub files_skipped: usize,
}

/// Compute a SHA-256 content hash for a code chunk.
fn content_hash(content: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(content.as_bytes());
    format!("{:x}", hasher.finalize())
}

/// Split a file's content into overlapping line-based chunks.
/// Returns (start_line, end_line, content) tuples, where end_line is inclusive.
fn chunk_content(content: &str, chunk_size: usize, overlap: usize) -> Vec<(i64, i64, String)> {
    let lines: Vec<&str> = content.lines().collect();
    if lines.is_empty() {
        return Vec::new();
    }

    let step = chunk_size.saturating_sub(overlap);
    if step == 0 {
        // Degenerate case: overlap >= chunk_size, chunk the whole file as one
        let end = lines.len() as i64;
        return vec![(1, end, content.to_string())];
    }

    let mut result = Vec::new();
    let mut start = 0;
    while start < lines.len() {
        let end = (start + chunk_size).min(lines.len());
        let chunk_lines = &lines[start..end];
        let chunk_text = chunk_lines.join("\n");

        // Lines are 1-indexed in source code, but our indices are 0-based
        let start_line = (start + 1) as i64;
        let end_line = end as i64;

        result.push((start_line, end_line, chunk_text));
        start += step;
    }

    result
}

/// Chunk all source files in the store and persist the chunks.
/// Skips files that are too large (above MAX_FILE_SIZE_BYTES), have no language
/// set, or whose chunks haven't changed since the last indexing (content hash match).
///
/// This runs after structural indexing and before embedding. It requires no LLM
/// calls — only filesystem reads and hash computation.
pub fn chunk_all_files(store: &Store, repo_root: &Path) -> Result<ChunkStats> {
    let files = store.get_files_for_chunking()?;

    let mut chunks_created = 0;
    let mut chunks_skipped_unchanged = 0;
    let mut files_processed = 0;
    let mut files_skipped = 0;

    for (file_id, path, language) in &files {
        // Skip files without a known language (binary, generated, etc.)
        let has_language = matches!(language, Some(lang) if !lang.is_empty());
        if !has_language {
            files_skipped += 1;
            continue;
        }

        let full_path = repo_root.join(path);
        let canonical_path = match full_path.canonicalize() {
            Ok(p) => p,
            Err(_) => {
                tracing::debug!(path = %path, "Skipping file that cannot be resolved");
                files_skipped += 1;
                continue;
            }
        };
        let canonical_root = match repo_root.canonicalize() {
            Ok(p) => p,
            Err(_) => {
                tracing::warn!(path = %path, "Cannot canonicalize repo root, skipping file");
                files_skipped += 1;
                continue;
            }
        };
        if !canonical_path.starts_with(&canonical_root) {
            tracing::warn!(path = %path, "Skipping file outside repo root");
            files_skipped += 1;
            continue;
        }

        let content = match std::fs::read_to_string(&canonical_path) {
            Ok(c) => c,
            Err(_) => {
                tracing::debug!(path = %path, "Skipping file that cannot be read");
                files_skipped += 1;
                continue;
            }
        };

        // Skip files that are too large
        if content.len() > limits::MAX_FILE_SIZE_BYTES {
            tracing::debug!(path = %path, size = content.len(), "Skipping oversized file for chunking");
            files_skipped += 1;
            continue;
        }

        let file_chunks = chunk_content(&content, chunks::CHUNK_SIZE, chunks::CHUNK_OVERLAP);
        if file_chunks.is_empty() {
            continue;
        }

        files_processed += 1;

        // Fetch existing chunks for this file to check for unchanged content.
        // If the file has changed at all, we delete old chunks + embeddings
        // and re-insert only the new ones. If nothing changed, we skip entirely.
        let file_chunks_existing = store.get_chunks_for_file(*file_id)?;
        let new_hashes: std::collections::HashSet<String> = file_chunks
            .iter()
            .map(|(_, _, text)| content_hash(text))
            .collect();
        let existing_hashes: std::collections::HashSet<String> = file_chunks_existing
            .iter()
            .map(|c| c.content_hash.clone())
            .collect();

        // If every new chunk's hash already exists in the file's existing chunks
        // and the count matches, the file is unchanged — skip entirely.
        let all_unchanged = new_hashes.len() == existing_hashes.len()
            && new_hashes.iter().all(|h| existing_hashes.contains(h));

        if all_unchanged {
            chunks_skipped_unchanged += file_chunks.len();
            continue;
        }

        // File has changed — delete old chunks and their embeddings, then insert fresh.
        store.delete_chunks_for_file(*file_id)?;

        for (start_line, end_line, chunk_text) in &file_chunks {
            let hash = content_hash(chunk_text);
            store.insert_code_chunk(*file_id, *start_line, *end_line, chunk_text, &hash)?;
            chunks_created += 1;
        }
    }

    tracing::info!(
        chunks_created,
        chunks_skipped_unchanged,
        files_processed,
        files_skipped,
        "Code chunking complete"
    );

    Ok(ChunkStats {
        chunks_created,
        chunks_skipped_unchanged,
        files_processed,
        files_skipped,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chunk_content_basic() {
        let content = "line1\nline2\nline3\nline4\nline5\nline6\nline7\nline8\nline9\nline10";
        let chunks = chunk_content(content, 5, 2);

        // 10 lines with chunk_size=5, overlap=2, step=3
        // Step = 5 - 2 = 3
        // Chunks: [1-5], [4-8], [7-10], [10-10]
        assert_eq!(chunks.len(), 4);
        assert_eq!(chunks[0].0, 1);
        assert_eq!(chunks[0].1, 5);
        assert_eq!(chunks[1].0, 4);
        assert_eq!(chunks[1].1, 8);
        assert_eq!(chunks[2].0, 7);
        assert_eq!(chunks[2].1, 10);
        assert_eq!(chunks[3].0, 10);
        assert_eq!(chunks[3].1, 10);
    }

    #[test]
    fn chunk_content_single_chunk() {
        let content = "line1\nline2\nline3";
        let chunks = chunk_content(content, 10, 2);

        // 3 lines with chunk_size=10: one chunk covering all lines
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0].0, 1);
        assert_eq!(chunks[0].1, 3);
    }

    #[test]
    fn chunk_content_empty() {
        let content = "";
        let chunks = chunk_content(content, 10, 2);
        assert!(chunks.is_empty());
    }

    #[test]
    fn content_hash_deterministic() {
        let hash1 = content_hash("fn main() {}");
        let hash2 = content_hash("fn main() {}");
        assert_eq!(hash1, hash2);

        let hash3 = content_hash("fn other() {}");
        assert_ne!(hash1, hash3);
    }

    #[test]
    fn chunk_content_overlap_preserves_content() {
        let content: Vec<String> = (1..=20).map(|i| format!("line {i}")).collect();
        let content_str = content.join("\n");
        let chunks = chunk_content(&content_str, 10, 3);

        // Verify overlap: the end of one chunk matches the start of the next
        assert!(chunks.len() > 1);
        for i in 1..chunks.len() {
            let prev_end = chunks[i - 1].2.clone();
            let curr_start = chunks[i].2.clone();

            // The last 3 lines of the previous chunk should be the first 3 lines of the current chunk
            let prev_lines: Vec<&str> = prev_end.lines().collect();
            let curr_lines: Vec<&str> = curr_start.lines().collect();
            assert_eq!(prev_lines[prev_lines.len() - 3..], curr_lines[..3]);
        }
    }
}
