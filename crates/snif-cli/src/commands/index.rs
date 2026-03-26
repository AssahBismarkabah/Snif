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

    let extractions = snif_parser::parse_repository(
        repo_path,
        &config.index.exclude_patterns,
    )?;

    let stats = snif_graph::build_graph(&store, &extractions)?;

    tracing::info!(
        files = stats.files_indexed,
        symbols = stats.symbols_extracted,
        imports = stats.imports_extracted,
        references = stats.references_extracted,
        "Index complete"
    );

    Ok(())
}
