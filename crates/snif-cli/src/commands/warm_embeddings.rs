use anyhow::{bail, Result};
use std::path::Path;

pub fn run(path: &str) -> Result<()> {
    let repo_path = Path::new(path);
    let config = snif_config::SnifConfig::load(repo_path)?;
    let embedding_cache_dir = config.resolved_embedding_cache_dir(repo_path);

    tracing::info!(
        cache_dir = %embedding_cache_dir.display(),
        "Warming embedding model cache"
    );

    match snif_embeddings::Embedder::new_with_cache_dir(&embedding_cache_dir) {
        Ok(_) => {
            tracing::info!(
                cache_dir = %embedding_cache_dir.display(),
                "Embedding model cache warmed"
            );
            println!(
                "Embedding model cache warmed at {}",
                embedding_cache_dir.display()
            );
            Ok(())
        }
        Err(error) if error.is_rate_limited() => bail!(
            "{} Cache directory: {}",
            error,
            embedding_cache_dir.display()
        ),
        Err(error) => Err(error.into()),
    }
}
