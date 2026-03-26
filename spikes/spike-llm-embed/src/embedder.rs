use anyhow::Result;
use fastembed::{EmbeddingModel, InitOptions, TextEmbedding};
use std::time::Instant;

pub struct Embedder {
    model: TextEmbedding,
}

pub struct EmbedResult {
    pub embeddings: Vec<Vec<f32>>,
    pub duration: std::time::Duration,
    pub dimension: usize,
}

impl Embedder {
    pub fn new() -> Result<Self> {
        println!("  Loading embedding model (AllMiniLML6V2)...");
        let start = Instant::now();
        let model = TextEmbedding::try_new(
            InitOptions::new(EmbeddingModel::AllMiniLML6V2).with_show_download_progress(true),
        )?;
        println!("  Model loaded in {:?}", start.elapsed());
        Ok(Self { model })
    }

    pub fn embed_batch(&self, texts: &[String]) -> Result<EmbedResult> {
        let start = Instant::now();
        let embeddings = self.model.embed(texts.to_vec(), None)?;
        let duration = start.elapsed();
        let dimension = embeddings.first().map(|e| e.len()).unwrap_or(0);

        Ok(EmbedResult {
            embeddings,
            duration,
            dimension,
        })
    }

    pub fn embed_single(&self, text: &str) -> Result<Vec<f32>> {
        let embeddings = self.model.embed(vec![text], None)?;
        Ok(embeddings.into_iter().next().unwrap())
    }
}
