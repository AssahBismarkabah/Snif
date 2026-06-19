use anyhow::Result;
use fastembed::{EmbeddingModel, InitOptions, TextEmbedding};
use snif_config::constants::{embeddings, model};
use snif_store::Store;
use std::collections::HashSet;
use std::error::Error;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

/// Runtime embedding model selection.
/// This must match `embeddings::MODEL_NAME` in snif-config.
/// Change here when switching embedding models.
const RUNTIME_MODEL: EmbeddingModel = EmbeddingModel::AllMiniLML6V2;

pub struct Embedder {
    model: TextEmbedding,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EmbedderLoadFailureKind {
    RateLimited,
    Other,
}

#[derive(Debug)]
pub struct EmbedderLoadError {
    kind: EmbedderLoadFailureKind,
    source: anyhow::Error,
}

impl EmbedderLoadError {
    fn new(source: anyhow::Error) -> Self {
        let kind = classify_embedder_load_error(&source);
        Self { kind, source }
    }

    pub fn kind(&self) -> EmbedderLoadFailureKind {
        self.kind
    }

    pub fn is_rate_limited(&self) -> bool {
        self.kind == EmbedderLoadFailureKind::RateLimited
    }
}

impl std::fmt::Display for EmbedderLoadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.kind {
            EmbedderLoadFailureKind::RateLimited => write!(
                f,
                "Embedding model download was rate-limited by Hugging Face/FastEmbed: {}. \
                 This is model acquisition, not the review LLM provider. Restore or cache the \
                 FastEmbed model cache, run `snif warm-embeddings`, or retry after the \
                 Hugging Face resolver rate-limit window resets.",
                self.source
            ),
            EmbedderLoadFailureKind::Other => {
                write!(f, "Failed to load embedding model: {}", self.source)
            }
        }
    }
}

impl Error for EmbedderLoadError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        Some(self.source.as_ref())
    }
}

pub struct EmbedStats {
    pub summaries_embedded: usize,
    pub dimension: usize,
    pub duration: Duration,
}

impl Embedder {
    /// Create a new Embedder instance with the configured embedding model.
    ///
    /// Model: all-MiniLM-L6-v2 (384 dimensions, ONNX via fastembed)
    /// See `RUNTIME_MODEL` constant - must match `embeddings::MODEL_NAME` in snif-config.
    pub fn new() -> std::result::Result<Self, EmbedderLoadError> {
        Self::new_with_cache_dir(PathBuf::from(embeddings::DEFAULT_CACHE_DIR))
    }

    pub fn new_with_cache_dir(
        cache_dir: impl Into<PathBuf>,
    ) -> std::result::Result<Self, EmbedderLoadError> {
        let cache_dir = cache_dir.into();
        tracing::info!("Loading embedding model ({})...", embeddings::MODEL_NAME);
        let start = Instant::now();
        let _env_lock = hf_home_lock()
            .lock()
            .expect("HF_HOME lock should not be poisoned");
        let _hf_home = HfHomeGuard::set(&cache_dir);
        let model = TextEmbedding::try_new(
            InitOptions::new(RUNTIME_MODEL)
                .with_cache_dir(cache_dir.clone())
                .with_show_download_progress(true),
        )
        .map_err(EmbedderLoadError::new)?;
        tracing::info!(elapsed = ?start.elapsed(), "Embedding model loaded");
        Ok(Self { model })
    }

    pub fn embed_single(&self, text: &str) -> Result<Vec<f32>> {
        let embeds = self.model.embed(vec![text], None)?;
        embeds
            .into_iter()
            .next()
            .ok_or_else(|| anyhow::anyhow!(embeddings::ERROR_EMPTY_EMBEDDING_RESULT))
    }

    pub fn embed_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>> {
        let embeddings = self.model.embed(texts.to_vec(), None)?;
        Ok(embeddings)
    }
}

fn hf_home_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

struct HfHomeGuard {
    previous: Option<OsString>,
}

impl HfHomeGuard {
    fn set(cache_dir: &Path) -> Self {
        let previous = std::env::var_os("HF_HOME");
        std::env::set_var("HF_HOME", cache_dir);
        Self { previous }
    }
}

impl Drop for HfHomeGuard {
    fn drop(&mut self) {
        match &self.previous {
            Some(value) => std::env::set_var("HF_HOME", value),
            None => std::env::remove_var("HF_HOME"),
        }
    }
}

fn classify_embedder_load_error(error: &anyhow::Error) -> EmbedderLoadFailureKind {
    let message = error
        .chain()
        .map(|cause| cause.to_string())
        .collect::<Vec<_>>()
        .join("\n")
        .to_ascii_lowercase();

    if message.contains("status code 429")
        || message.contains("429 too many requests")
        || message.contains("too many requests")
    {
        EmbedderLoadFailureKind::RateLimited
    } else {
        EmbedderLoadFailureKind::Other
    }
}

pub fn embed_all_summaries(store: &Store, embedder: &Embedder) -> Result<EmbedStats> {
    let start = Instant::now();
    let summaries = store.get_all_summaries()?;

    if summaries.is_empty() {
        tracing::info!("No summaries found, skipping embedding");
        return Ok(EmbedStats {
            summaries_embedded: embeddings::DEFAULT_COUNT,
            dimension: model::DEFAULT_EMBEDDING_DIMENSION,
            duration: start.elapsed(),
        });
    }

    // Filter out summaries that already have embeddings
    let existing_ids: HashSet<i64> = store.get_embedded_summary_ids()?.into_iter().collect();
    let summaries: Vec<_> = summaries
        .into_iter()
        .filter(|(id, _)| !existing_ids.contains(id))
        .collect();

    if summaries.is_empty() {
        tracing::info!("All summaries already embedded, skipping");
        return Ok(EmbedStats {
            summaries_embedded: embeddings::DEFAULT_COUNT,
            dimension: model::DEFAULT_EMBEDDING_DIMENSION,
            duration: start.elapsed(),
        });
    }

    tracing::info!(
        count = summaries.len(),
        skipped = existing_ids.len(),
        "Embedding new summaries"
    );

    // Batch in groups of configured batch size
    let batch_size = embeddings::BATCH_SIZE;
    let mut total_embedded = embeddings::INITIAL_TOTAL;
    let mut dimension = model::DEFAULT_EMBEDDING_DIMENSION;

    for chunk in summaries.chunks(batch_size) {
        let ids: Vec<i64> = chunk.iter().map(|(id, _)| *id).collect();
        let texts: Vec<String> = chunk.iter().map(|(_, text)| text.clone()).collect();

        let embeddings = embedder.embed_batch(&texts)?;
        if let Some(first) = embeddings.first() {
            dimension = first.len();
        }

        let entries: Vec<(i64, Vec<f32>)> = ids.into_iter().zip(embeddings).collect();

        store.insert_summary_embeddings_batch(&entries)?;
        total_embedded += entries.len();

        tracing::debug!(batch = total_embedded, "Embedded batch");
    }

    Ok(EmbedStats {
        summaries_embedded: total_embedded,
        dimension,
        duration: start.elapsed(),
    })
}

/// Check whether any summaries in the store are missing embeddings.
/// Used by the review command to decide whether on-demand embedding is needed
/// after on-demand summarization.
pub fn has_summaries_missing_embeddings(store: &Store) -> Result<bool> {
    let summaries = store.get_all_summaries()?;
    if summaries.is_empty() {
        return Ok(false);
    }

    let embedded_ids: HashSet<i64> = store.get_embedded_summary_ids()?.into_iter().collect();
    Ok(summaries
        .iter()
        .any(|(summary_id, _)| !embedded_ids.contains(summary_id)))
}

/// Embed all code chunks that don't already have embeddings.
/// This runs after chunking and requires no LLM calls — only the local
/// embedding model is used. Returns stats about the embedding operation.
pub fn embed_all_code_chunks(store: &Store, embedder: &Embedder) -> Result<EmbedStats> {
    let start = Instant::now();

    let unembedded = store.get_unembedded_chunks()?;
    if unembedded.is_empty() {
        tracing::info!("All code chunks already embedded, skipping");
        return Ok(EmbedStats {
            summaries_embedded: embeddings::DEFAULT_COUNT,
            dimension: model::DEFAULT_EMBEDDING_DIMENSION,
            duration: start.elapsed(),
        });
    }

    tracing::info!(count = unembedded.len(), "Embedding code chunks");

    let batch_size = embeddings::BATCH_SIZE;
    let mut total_embedded = embeddings::INITIAL_TOTAL;
    let mut dimension = model::DEFAULT_EMBEDDING_DIMENSION;

    for chunk in unembedded.chunks(batch_size) {
        let ids: Vec<i64> = chunk.iter().map(|c| c.id).collect();
        let texts: Vec<String> = chunk.iter().map(|c| c.content.clone()).collect();

        let embeddings = embedder.embed_batch(&texts)?;
        if let Some(first) = embeddings.first() {
            dimension = first.len();
        }

        let entries: Vec<(i64, Vec<f32>)> = ids.into_iter().zip(embeddings).collect();

        store.insert_code_embeddings_batch(&entries)?;
        total_embedded += entries.len();

        tracing::debug!(batch = total_embedded, "Embedded code chunk batch");
    }

    Ok(EmbedStats {
        summaries_embedded: total_embedded,
        dimension,
        duration: start.elapsed(),
    })
}

/// Check whether any code chunks in the store are missing embeddings.
/// Used to decide whether on-demand code embedding is needed during review.
pub fn has_code_chunks_missing_embeddings(store: &Store) -> Result<bool> {
    let unembedded = store.get_unembedded_chunks()?;
    Ok(!unembedded.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifier_detects_status_code_429() {
        let error =
            anyhow::anyhow!("request error: https://huggingface.co/model.onnx: status code 429");

        assert_eq!(
            classify_embedder_load_error(&error),
            EmbedderLoadFailureKind::RateLimited
        );
    }

    #[test]
    fn classifier_detects_too_many_requests() {
        let error = anyhow::anyhow!("429 Too Many Requests for model.onnx");

        assert_eq!(
            classify_embedder_load_error(&error),
            EmbedderLoadFailureKind::RateLimited
        );
    }

    #[test]
    fn classifier_keeps_non_429_failures_generic() {
        let error = anyhow::anyhow!("failed to parse tokenizer.json");

        assert_eq!(
            classify_embedder_load_error(&error),
            EmbedderLoadFailureKind::Other
        );
    }

    #[test]
    fn hf_home_guard_restores_previous_value() {
        let _lock = hf_home_lock()
            .lock()
            .expect("HF_HOME lock should not be poisoned");
        let original = std::env::var_os("HF_HOME");
        std::env::set_var("HF_HOME", "/tmp/original-hf-home");

        {
            let _guard = HfHomeGuard::set(Path::new("/tmp/snif-fastembed-cache"));
            assert_eq!(
                std::env::var_os("HF_HOME"),
                Some(OsString::from("/tmp/snif-fastembed-cache"))
            );
        }

        assert_eq!(
            std::env::var_os("HF_HOME"),
            Some(OsString::from("/tmp/original-hf-home"))
        );
        match original {
            Some(value) => std::env::set_var("HF_HOME", value),
            None => std::env::remove_var("HF_HOME"),
        }
    }
}
