use std::collections::HashSet;
use std::path::Path;

use anyhow::Result;
use snif_config::constants::retrieval;

pub fn run(path: &str, full: bool) -> Result<()> {
    let repo_path = Path::new(path);
    tracing::info!(path = %repo_path.display(), full, "Starting index");

    let config = snif_config::SnifConfig::load(repo_path)?;

    let store = snif_store::Store::open(Path::new(&config.index.db_path))?;

    if full {
        store.reset_schema()?;
    }

    // Step 3: Parse repository and build structural graph
    let extractions = snif_parser::parse_repository(repo_path, &config.index.exclude_patterns)?;

    let graph_stats = snif_graph::build_graph(&store, &extractions)?;

    tracing::info!(
        files = graph_stats.files_indexed,
        skipped = graph_stats.files_skipped,
        symbols = graph_stats.symbols_extracted,
        imports = graph_stats.imports_extracted,
        references = graph_stats.references_extracted,
        "Structural graph built"
    );

    // Step 4: Co-change analysis from git history
    let cochange_stats =
        snif_cochange::analyze_cochange(&store, repo_path, retrieval::MIN_COCHANGE_CORRELATION, 3)?;

    tracing::info!(
        commits = cochange_stats.commits_analyzed,
        pairs = cochange_stats.pairs_stored,
        "Co-change analysis complete"
    );

    // Step 5: LLM summary generation
    let summary_stats = snif_summarizer::summarize_all(
        &store,
        repo_path,
        &config.model,
        config.context.summarizer_concurrency,
    )?;

    tracing::info!(
        symbols = summary_stats.symbols_summarized,
        files = summary_stats.files_summarized,
        errors = summary_stats.errors,
        rate_limited = summary_stats.rate_limited,
        duration = ?summary_stats.total_duration,
        "Summarization complete"
    );

    // Step 6: Vector embeddings
    if has_summaries_missing_embeddings(&store)? {
        let embedding_cache_dir = config.resolved_embedding_cache_dir(repo_path);
        match snif_embeddings::Embedder::new_with_cache_dir(&embedding_cache_dir) {
            Ok(embedder) => {
                let embed_stats = snif_embeddings::embed_all_summaries(&store, &embedder)?;

                tracing::info!(
                    embedded = embed_stats.summaries_embedded,
                    dimension = embed_stats.dimension,
                    duration = ?embed_stats.duration,
                    "Embedding complete"
                );
            }
            Err(error) if error.is_rate_limited() => {
                tracing::warn!(
                    cache_dir = %embedding_cache_dir.display(),
                    error = %error,
                    "Skipping summary embeddings because the embedding model download was rate-limited"
                );
                tracing::warn!(
                    "Semantic indexing is incomplete until the FastEmbed model cache is restored or warmed"
                );
            }
            Err(error) => return Err(error.into()),
        }
    } else {
        tracing::info!("No summaries need embedding, skipping embedding model load");
    }

    tracing::info!("Index complete");
    Ok(())
}

fn has_summaries_missing_embeddings(store: &snif_store::Store) -> Result<bool> {
    let summaries = store.get_all_summaries()?;
    if summaries.is_empty() {
        return Ok(false);
    }

    let embedded_ids: HashSet<i64> = store.get_embedded_summary_ids()?.into_iter().collect();
    Ok(summaries
        .iter()
        .any(|(summary_id, _)| !embedded_ids.contains(summary_id)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use snif_config::constants::{model, summarizer};

    #[test]
    fn detects_cached_summary_missing_embedding() {
        let store = snif_store::Store::open_in_memory().expect("store should open");
        store
            .insert_summary(None, None, summarizer::KIND_FUNCTION, "summary", Some(1))
            .expect("summary should insert");

        assert!(has_summaries_missing_embeddings(&store).expect("check should succeed"));
    }

    #[test]
    fn skips_embedding_load_when_all_cached_summaries_are_embedded() {
        let store = snif_store::Store::open_in_memory().expect("store should open");
        let summary_id = store
            .insert_summary(None, None, summarizer::KIND_FUNCTION, "summary", Some(1))
            .expect("summary should insert");
        store
            .insert_summary_embeddings_batch(&[(
                summary_id,
                vec![0.0; model::DEFAULT_EMBEDDING_DIMENSION],
            )])
            .expect("embedding should insert");

        assert!(!has_summaries_missing_embeddings(&store).expect("check should succeed"));
    }

    #[test]
    fn skips_embedding_load_when_no_summaries_exist() {
        let store = snif_store::Store::open_in_memory().expect("store should open");

        assert!(!has_summaries_missing_embeddings(&store).expect("check should succeed"));
    }
}
