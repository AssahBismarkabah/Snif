use anyhow::Result;
use snif_embeddings::Embedder;
use snif_store::Store;
use snif_types::RetrievalMethod;
use std::collections::HashMap;

pub fn semantic_retrieval(
    store: &Store,
    changed_file_ids: &[i64],
    embedder: &Embedder,
    k: usize,
) -> Result<HashMap<i64, Vec<RetrievalMethod>>> {
    let mut results: HashMap<i64, Vec<RetrievalMethod>> = HashMap::new();

    for file_id in changed_file_ids {
        let summary = match store.get_summary_for_file(*file_id)? {
            Some((_, text)) => text,
            None => continue,
        };

        let query_embedding = match embedder.embed_single(&summary) {
            Ok(e) => e,
            Err(_) => continue,
        };

        let similar = store.query_similar_summaries_with_details(&query_embedding, k)?;

        for entry in similar {
            let candidate_file_id = match entry.file_id {
                Some(fid) => fid,
                None => continue,
            };

            if changed_file_ids.contains(&candidate_file_id) {
                continue;
            }

            results
                .entry(candidate_file_id)
                .or_default()
                .push(RetrievalMethod::Semantic {
                    distance: entry.distance,
                });
        }
    }

    Ok(results)
}
