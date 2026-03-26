mod keyword;
mod semantic;
mod structural;

use anyhow::Result;
use snif_config::RetrievalWeights;
use snif_embeddings::Embedder;
use snif_store::Store;
use snif_types::{RetrievalMethod, RetrievalResult, RetrievalResults, StructuralReason};
use std::collections::HashMap;

pub fn retrieve(
    store: &Store,
    changed_paths: &[String],
    diff_identifiers: &[String],
    embedder: &Embedder,
    weights: &RetrievalWeights,
) -> Result<RetrievalResults> {
    let mut changed_file_ids: Vec<(i64, String)> = Vec::new();
    for path in changed_paths {
        if let Some(id) = store.get_file_id(path)? {
            changed_file_ids.push((id, path.clone()));
        }
    }

    let changed_ids: Vec<i64> = changed_file_ids.iter().map(|(id, _)| *id).collect();

    let struct_results = structural::structural_retrieval(store, &changed_file_ids)?;
    let sem_results = semantic::semantic_retrieval(store, &changed_ids, embedder, 20)?;
    let kw_results = keyword::keyword_retrieval(store, diff_identifiers, &changed_ids)?;

    let structural_count = struct_results.len();
    let semantic_count = sem_results.len();
    let keyword_count = kw_results.len();

    let mut merged: HashMap<i64, Vec<RetrievalMethod>> = HashMap::new();

    for (file_id, methods) in struct_results {
        merged.entry(file_id).or_default().extend(methods);
    }
    for (file_id, methods) in sem_results {
        merged.entry(file_id).or_default().extend(methods);
    }
    for (file_id, methods) in kw_results {
        merged.entry(file_id).or_default().extend(methods);
    }

    let file_paths = store.get_all_file_paths()?;
    let path_map: HashMap<i64, String> = file_paths.into_iter().collect();

    let mut results: Vec<RetrievalResult> = merged
        .into_iter()
        .filter_map(|(file_id, sources)| {
            let path = path_map.get(&file_id)?.clone();
            let score = compute_score(&sources, weights);
            Some(RetrievalResult {
                file_id,
                path,
                score,
                sources,
            })
        })
        .collect();

    results.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));

    Ok(RetrievalResults {
        results,
        structural_count,
        semantic_count,
        keyword_count,
    })
}

fn compute_score(sources: &[RetrievalMethod], weights: &RetrievalWeights) -> f64 {
    let mut score = 0.0;

    for source in sources {
        match source {
            RetrievalMethod::Structural(reason) => {
                let method_score = match reason {
                    StructuralReason::DirectImport => 1.0,
                    StructuralReason::ReverseImport => 0.8,
                    StructuralReason::CoChange { correlation } => *correlation,
                    StructuralReason::SymbolReference { .. } => 0.6,
                };
                score += weights.structural * method_score;
            }
            RetrievalMethod::Semantic { distance } => {
                let similarity = (1.0 - distance).max(0.0);
                score += weights.semantic * similarity;
            }
            RetrievalMethod::Keyword { matched_terms } => {
                let term_score = (matched_terms.len() as f64).min(3.0) / 3.0;
                score += weights.keyword * term_score;
            }
        }
    }

    score
}
