use anyhow::Result;
use fastembed::{EmbeddingModel, InitOptions, TextEmbedding};
use snif_config::constants::model;
use snif_store::Store;
use std::collections::HashSet;
use std::time::{Duration, Instant};

pub struct Embedder {
    model: TextEmbedding,
}

pub struct EmbedStats {
    pub summaries_embedded: usize,
    pub dimension: usize,
    pub duration: Duration,
}

impl Embedder {
    pub fn new() -> Result<Self> {
        tracing::info!("Loading embedding model (AllMiniLML6V2)...");
        let start = Instant::now();
        let model = TextEmbedding::try_new(
            InitOptions::new(EmbeddingModel::AllMiniLML6V2).with_show_download_progress(true),
        )?;
        tracing::info!(elapsed = ?start.elapsed(), "Embedding model loaded");
        Ok(Self { model })
    }

    pub fn embed_single(&self, text: &str) -> Result<Vec<f32>> {
        let embeddings = self.model.embed(vec![text], None)?;
        embeddings
            .into_iter()
            .next()
            .ok_or_else(|| anyhow::anyhow!("Embedding model returned empty result for text"))
    }

    pub fn embed_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>> {
        let embeddings = self.model.embed(texts.to_vec(), None)?;
        Ok(embeddings)
    }
}

pub fn embed_all_summaries(store: &Store, embedder: &Embedder) -> Result<EmbedStats> {
    let start = Instant::now();
    let summaries = store.get_all_summaries()?;

    if summaries.is_empty() {
        return Ok(EmbedStats {
            summaries_embedded: 0,
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
            summaries_embedded: 0,
            dimension: 384,
            duration: start.elapsed(),
        });
    }

    tracing::info!(
        count = summaries.len(),
        skipped = existing_ids.len(),
        "Embedding new summaries"
    );

    // Batch in groups of 64
    let batch_size = 64;
    let mut total_embedded = 0;
    let mut dimension = 384;

    for chunk in summaries.chunks(batch_size) {
        let ids: Vec<i64> = chunk.iter().map(|(id, _)| *id).collect();
        let texts: Vec<String> = chunk.iter().map(|(_, text)| text.clone()).collect();

        let embeddings = embedder.embed_batch(&texts)?;
        if let Some(first) = embeddings.first() {
            dimension = first.len();
        }

        let entries: Vec<(i64, Vec<f32>)> = ids.into_iter().zip(embeddings.into_iter()).collect();

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
