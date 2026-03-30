use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct SnifConfig {
    pub platform: PlatformConfig,
    pub model: ModelConfig,
    pub index: IndexConfig,
    pub context: ContextConfig,
    pub filter: FilterConfig,
    pub conventions_paths: Vec<String>,
    pub eval_fixtures_path: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct PlatformConfig {
    pub provider: String,
    pub api_base: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct ModelConfig {
    pub review_model: String,
    pub summary_model: String,
    pub endpoint: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct IndexConfig {
    pub db_path: String,
    pub embedding_dimension: usize,
    pub languages: Vec<String>,
    pub exclude_patterns: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ContextConfig {
    pub max_tokens: usize,
    pub max_files: usize,
    pub retrieval_weights: RetrievalWeights,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct RetrievalWeights {
    pub structural: f64,
    pub semantic: f64,
    pub keyword: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct FilterConfig {
    pub min_confidence: f64,
    pub suppress_style_only: bool,
    pub feedback_min_signals: usize,
}

impl Default for PlatformConfig {
    fn default() -> Self {
        Self {
            provider: "github".to_string(),
            api_base: None,
        }
    }
}

impl Default for IndexConfig {
    fn default() -> Self {
        Self {
            db_path: ".snif/index.db".to_string(),
            embedding_dimension: 384,
            languages: vec![
                "rust".to_string(),
                "typescript".to_string(),
                "python".to_string(),
            ],
            exclude_patterns: vec![
                "target".to_string(),
                "node_modules".to_string(),
                "vendor".to_string(),
                ".git".to_string(),
            ],
        }
    }
}

impl Default for ContextConfig {
    fn default() -> Self {
        Self {
            max_tokens: 128_000,
            max_files: 50,
            retrieval_weights: RetrievalWeights::default(),
        }
    }
}

impl Default for RetrievalWeights {
    fn default() -> Self {
        Self {
            structural: 1.0,
            semantic: 0.7,
            keyword: 0.3,
        }
    }
}

impl Default for FilterConfig {
    fn default() -> Self {
        Self {
            min_confidence: 0.7,
            suppress_style_only: true,
            feedback_min_signals: 20,
        }
    }
}

impl SnifConfig {
    pub fn load(repo_root: &Path) -> Result<Self> {
        let config_path = repo_root.join(".snif.json");

        let mut config = if config_path.exists() {
            let content =
                std::fs::read_to_string(&config_path).context("Failed to read .snif.json")?;
            serde_json::from_str(&content).context("Failed to parse .snif.json")?
        } else {
            Self::default()
        };

        config.merge_env_vars();
        Ok(config)
    }

    fn merge_env_vars(&mut self) {
        if let Ok(val) = std::env::var("SNIF_ENDPOINT") {
            self.model.endpoint = val;
        }
        if let Ok(val) = std::env::var("SNIF_DB_PATH") {
            self.index.db_path = val;
        }
    }
}
