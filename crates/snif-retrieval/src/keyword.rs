use anyhow::Result;
use snif_store::Store;
use snif_types::RetrievalMethod;
use std::collections::{HashMap, HashSet};

pub fn keyword_retrieval(
    store: &Store,
    identifiers: &[String],
    exclude_file_ids: &[i64],
) -> Result<HashMap<i64, Vec<RetrievalMethod>>> {
    let mut results: HashMap<i64, Vec<RetrievalMethod>> = HashMap::new();

    if identifiers.is_empty() {
        return Ok(results);
    }

    let exclude_set: HashSet<i64> = exclude_file_ids.iter().copied().collect();

    let symbol_matches = store.get_files_defining_symbols(identifiers)?;
    for (file_id, name) in &symbol_matches {
        if !exclude_set.contains(file_id) {
            results
                .entry(*file_id)
                .or_default()
                .push(RetrievalMethod::Keyword {
                    matched_terms: vec![name.clone()],
                });
        }
    }

    let ref_matches = store.get_files_referencing_symbols(identifiers)?;
    for (file_id, name) in &ref_matches {
        if !exclude_set.contains(file_id) {
            results
                .entry(*file_id)
                .or_default()
                .push(RetrievalMethod::Keyword {
                    matched_terms: vec![name.clone()],
                });
        }
    }

    Ok(results)
}
