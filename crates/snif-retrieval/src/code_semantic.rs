use anyhow::Result;
use snif_config::constants::retrieval;
use snif_embeddings::Embedder;
use snif_store::Store;
use snif_types::RetrievalMethod;
use std::collections::{HashMap, HashSet};

/// Code-semantic retrieval: embeds the full source of changed files and finds
/// similar files via code-chunk embeddings. This is a fallback for files that
/// lack LLM summaries — raw code embeddings are noisier than summary embeddings
/// but provide coverage immediately after structural indexing with no LLM calls.
pub fn code_semantic_retrieval(
    store: &Store,
    changed_file_ids: &[i64],
    embedder: &Embedder,
    k: usize,
) -> Result<HashMap<i64, Vec<RetrievalMethod>>> {
    let mut results: HashMap<i64, Vec<RetrievalMethod>> = HashMap::new();
    let changed_ids_set: HashSet<i64> = changed_file_ids.iter().copied().collect();

    for file_id in changed_file_ids {
        // Get all chunks for this changed file
        let chunks = match store.get_chunks_for_file(*file_id) {
            Ok(chunks) if !chunks.is_empty() => chunks,
            Ok(_) => continue,
            Err(_) => continue,
        };

        // Concatenate chunk content to form a single query vector per file.
        // Using the concatenated source preserves more context than embedding
        // individual chunks separately for the query side.
        let combined_content: String = chunks
            .iter()
            .map(|c| c.content.as_str())
            .collect::<Vec<_>>()
            .join("\n");

        if combined_content.is_empty() {
            continue;
        }

        let query_embedding = match embedder.embed_single(&combined_content) {
            Ok(e) => e,
            Err(_) => continue,
        };

        let similar = store.query_similar_code_chunks(&query_embedding, k)?;

        // De-duplicate by file_id, keeping the best (lowest) distance per file
        let mut best_by_file: HashMap<i64, f64> = HashMap::new();
        for entry in similar {
            if changed_ids_set.contains(&entry.file_id) {
                continue;
            }
            best_by_file
                .entry(entry.file_id)
                .and_modify(|d| {
                    if entry.distance < *d {
                        *d = entry.distance;
                    }
                })
                .or_insert(entry.distance);
        }

        for (result_file_id, best_distance) in best_by_file {
            // Apply the similarity floor so negative distances don't inflate scores
            let similarity = (1.0 - best_distance).max(retrieval::SEMANTIC_SIMILARITY_FLOOR);
            // Skip results with zero or negative effective similarity
            if similarity <= 0.0 {
                continue;
            }
            results
                .entry(result_file_id)
                .or_default()
                .push(RetrievalMethod::CodeSemantic {
                    distance: best_distance,
                });
        }
    }

    Ok(results)
}
