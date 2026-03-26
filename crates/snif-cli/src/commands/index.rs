use anyhow::Result;
use std::path::Path;

pub fn run(path: &str, full: bool) -> Result<()> {
    let repo_path = Path::new(path);
    tracing::info!(path = %repo_path.display(), full, "Starting index");

    let config = snif_config::SnifConfig::load(repo_path)?;

    let store = snif_store::Store::open(Path::new(&config.index.db_path))?;

    if full {
        store.reset_schema()?;
    }

    // Step 3: Parse repository and build structural graph
    let extractions = snif_parser::parse_repository(
        repo_path,
        &config.index.exclude_patterns,
    )?;

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
    let cochange_stats = snif_cochange::analyze_cochange(&store, repo_path, 0.1, 3)?;

    tracing::info!(
        commits = cochange_stats.commits_analyzed,
        pairs = cochange_stats.pairs_stored,
        "Co-change analysis complete"
    );

    // Step 5: LLM summary generation
    let summary_stats = snif_summarizer::summarize_all(&store, repo_path, &config.model)?;

    tracing::info!(
        symbols = summary_stats.symbols_summarized,
        files = summary_stats.files_summarized,
        errors = summary_stats.errors,
        duration = ?summary_stats.total_duration,
        "Summarization complete"
    );

    // Step 6: Vector embeddings
    if summary_stats.symbols_summarized > 0 || summary_stats.files_summarized > 0 {
        let embedder = snif_embeddings::Embedder::new()?;
        let embed_stats = snif_embeddings::embed_all_summaries(&store, &embedder)?;

        tracing::info!(
            embedded = embed_stats.summaries_embedded,
            dimension = embed_stats.dimension,
            duration = ?embed_stats.duration,
            "Embedding complete"
        );
    }

    tracing::info!("Index complete");
    Ok(())
}
