pub mod constants;
pub mod env;
pub mod formatters;

use anyhow::{Context, Result};
use constants::embeddings;
use constants::model;
use constants::thresholds;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct SnifConfig {
    pub platform: PlatformConfig,
    pub model: ModelConfig,
    pub index: IndexConfig,
    pub context: ContextConfig,
    pub review: ReviewConfig,
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
    pub embedding_cache_dir: String,
    pub embedding_dimension: usize,
    pub languages: Vec<String>,
    pub exclude_patterns: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ContextConfig {
    pub max_tokens: usize,
    pub max_files: usize,
    pub output_reserve_tokens: usize,
    pub summarizer_concurrency: usize,
    pub retrieval_weights: RetrievalWeights,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ReviewConfig {
    pub inconclusive_mode: ReviewInconclusiveMode,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReviewInconclusiveMode {
    #[default]
    Fail,
    Warn,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct RetrievalWeights {
    pub structural: f64,
    pub semantic: f64,
    pub code_semantic: f64,
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
            embedding_cache_dir: embeddings::DEFAULT_CACHE_DIR.to_string(),
            embedding_dimension: model::DEFAULT_EMBEDDING_DIMENSION,
            languages: vec![
                "rust".to_string(),
                "typescript".to_string(),
                "python".to_string(),
                "java".to_string(),
            ],
            exclude_patterns: vec![
                "target".to_string(),
                "node_modules".to_string(),
                "vendor".to_string(),
                ".git".to_string(),
                "build".to_string(),
                ".gradle".to_string(),
                ".mvn".to_string(),
            ],
        }
    }
}

impl Default for ContextConfig {
    fn default() -> Self {
        Self {
            max_tokens: model::DEFAULT_MAX_TOKENS,
            max_files: model::DEFAULT_MAX_FILES,
            output_reserve_tokens: model::DEFAULT_OUTPUT_RESERVE_TOKENS,
            summarizer_concurrency: model::DEFAULT_SUMMARIZER_CONCURRENCY,
            retrieval_weights: RetrievalWeights::default(),
        }
    }
}

impl Default for RetrievalWeights {
    fn default() -> Self {
        Self {
            structural: 1.0,
            semantic: 0.7,
            code_semantic: 0.4,
            keyword: 0.3,
        }
    }
}

impl Default for ReviewConfig {
    fn default() -> Self {
        Self {
            inconclusive_mode: ReviewInconclusiveMode::Fail,
        }
    }
}

impl ReviewInconclusiveMode {
    pub fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "fail" => Some(Self::Fail),
            "warn" => Some(Self::Warn),
            _ => None,
        }
    }
}

impl Default for FilterConfig {
    fn default() -> Self {
        Self {
            min_confidence: thresholds::MIN_CONFIDENCE_DEFAULT,
            suppress_style_only: true,
            feedback_min_signals: thresholds::FEEDBACK_MIN_SIGNALS,
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

    pub fn default_with_env() -> Self {
        let mut config = Self::default();
        config.merge_env_vars();
        config
    }

    fn merge_env_vars(&mut self) {
        if let Ok(val) = std::env::var(env::app::SNIF_ENDPOINT) {
            self.model.endpoint = val;
        }
        if let Ok(val) = std::env::var(env::app::SNIF_DB_PATH) {
            self.index.db_path = val;
        }
        if let Ok(val) = std::env::var(env::app::FASTEMBED_CACHE_DIR) {
            self.index.embedding_cache_dir = val;
        }
        if let Ok(val) = std::env::var(env::app::SNIF_EMBEDDING_CACHE_DIR) {
            self.index.embedding_cache_dir = val;
        }
        if let Ok(val) = std::env::var(env::app::SNIF_REVIEW_INCONCLUSIVE_MODE) {
            if let Some(mode) = ReviewInconclusiveMode::parse(&val) {
                self.review.inconclusive_mode = mode;
            }
        }
    }

    pub fn resolved_embedding_cache_dir(&self, repo_root: &Path) -> PathBuf {
        let cache_dir = PathBuf::from(&self.index.embedding_cache_dir);
        if cache_dir.is_absolute() {
            cache_dir
        } else {
            repo_root.join(cache_dir)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Mutex, OnceLock};

    fn env_lock() -> &'static Mutex<()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
    }

    #[test]
    fn default_context_sets_summarizer_concurrency_to_current_behavior() {
        let config = SnifConfig::default();

        assert_eq!(config.context.summarizer_concurrency, 3);
    }

    #[test]
    fn missing_summarizer_concurrency_uses_default() {
        let config: SnifConfig = serde_json::from_str(
            r#"{
                "context": {
                    "max_tokens": 64000
                }
            }"#,
        )
        .expect("config should parse");

        assert_eq!(config.context.max_tokens, 64000);
        assert_eq!(config.context.summarizer_concurrency, 3);
    }

    #[test]
    fn explicit_summarizer_concurrency_is_honored() {
        let config: SnifConfig = serde_json::from_str(
            r#"{
                "context": {
                    "summarizer_concurrency": 1
                }
            }"#,
        )
        .expect("config should parse");

        assert_eq!(config.context.summarizer_concurrency, 1);
    }

    #[test]
    fn default_review_inconclusive_mode_fails() {
        let config = SnifConfig::default();

        assert_eq!(
            config.review.inconclusive_mode,
            ReviewInconclusiveMode::Fail
        );
    }

    #[test]
    fn missing_review_config_uses_default() {
        let config: SnifConfig = serde_json::from_str(
            r#"{
                "context": {
                    "max_tokens": 64000
                }
            }"#,
        )
        .expect("config should parse");

        assert_eq!(
            config.review.inconclusive_mode,
            ReviewInconclusiveMode::Fail
        );
    }

    #[test]
    fn explicit_review_inconclusive_warn_mode_is_honored() {
        let config: SnifConfig = serde_json::from_str(
            r#"{
                "review": {
                    "inconclusive_mode": "warn"
                }
            }"#,
        )
        .expect("config should parse");

        assert_eq!(
            config.review.inconclusive_mode,
            ReviewInconclusiveMode::Warn
        );
    }

    #[test]
    fn default_embedding_cache_dir_matches_fastembed_default() {
        let config = SnifConfig::default();

        assert_eq!(config.index.embedding_cache_dir, ".fastembed_cache");
    }

    #[test]
    fn explicit_embedding_cache_dir_is_honored() {
        let config: SnifConfig = serde_json::from_str(
            r#"{
                "index": {
                    "embedding_cache_dir": ".cache/snif-embeddings"
                }
            }"#,
        )
        .expect("config should parse");

        assert_eq!(config.index.embedding_cache_dir, ".cache/snif-embeddings");
    }

    #[test]
    fn relative_embedding_cache_dir_resolves_from_repo_root() {
        let config = SnifConfig::default();

        assert_eq!(
            config.resolved_embedding_cache_dir(Path::new("/repo")),
            PathBuf::from("/repo/.fastembed_cache")
        );
    }

    #[test]
    fn absolute_embedding_cache_dir_is_preserved() {
        let mut config = SnifConfig::default();
        config.index.embedding_cache_dir = "/tmp/snif-fastembed".to_string();

        assert_eq!(
            config.resolved_embedding_cache_dir(Path::new("/repo")),
            PathBuf::from("/tmp/snif-fastembed")
        );
    }

    #[test]
    fn fastembed_cache_dir_env_overrides_json() {
        let _guard = env_lock().lock().expect("env lock should not be poisoned");
        let old_fastembed = std::env::var(env::app::FASTEMBED_CACHE_DIR).ok();
        let old_snif = std::env::var(env::app::SNIF_EMBEDDING_CACHE_DIR).ok();
        std::env::remove_var(env::app::SNIF_EMBEDDING_CACHE_DIR);
        std::env::set_var(env::app::FASTEMBED_CACHE_DIR, "/tmp/fastembed-cache");

        let mut config: SnifConfig = serde_json::from_str(
            r#"{
                "index": {
                    "embedding_cache_dir": ".json-cache"
                }
            }"#,
        )
        .expect("config should parse");
        config.merge_env_vars();

        assert_eq!(config.index.embedding_cache_dir, "/tmp/fastembed-cache");

        restore_env_var(env::app::FASTEMBED_CACHE_DIR, old_fastembed);
        restore_env_var(env::app::SNIF_EMBEDDING_CACHE_DIR, old_snif);
    }

    #[test]
    fn snif_embedding_cache_dir_env_overrides_fastembed_env() {
        let _guard = env_lock().lock().expect("env lock should not be poisoned");
        let old_fastembed = std::env::var(env::app::FASTEMBED_CACHE_DIR).ok();
        let old_snif = std::env::var(env::app::SNIF_EMBEDDING_CACHE_DIR).ok();
        std::env::set_var(env::app::FASTEMBED_CACHE_DIR, "/tmp/fastembed-cache");
        std::env::set_var(env::app::SNIF_EMBEDDING_CACHE_DIR, "/tmp/snif-cache");

        let mut config = SnifConfig::default();
        config.merge_env_vars();

        assert_eq!(config.index.embedding_cache_dir, "/tmp/snif-cache");

        restore_env_var(env::app::FASTEMBED_CACHE_DIR, old_fastembed);
        restore_env_var(env::app::SNIF_EMBEDDING_CACHE_DIR, old_snif);
    }

    #[test]
    fn review_inconclusive_mode_env_overrides_json() {
        let _guard = env_lock().lock().expect("env lock should not be poisoned");
        let old_mode = std::env::var(env::app::SNIF_REVIEW_INCONCLUSIVE_MODE).ok();
        std::env::set_var(env::app::SNIF_REVIEW_INCONCLUSIVE_MODE, "warn");

        let mut config: SnifConfig = serde_json::from_str(
            r#"{
                "review": {
                    "inconclusive_mode": "fail"
                }
            }"#,
        )
        .expect("config should parse");
        config.merge_env_vars();

        assert_eq!(
            config.review.inconclusive_mode,
            ReviewInconclusiveMode::Warn
        );

        restore_env_var(env::app::SNIF_REVIEW_INCONCLUSIVE_MODE, old_mode);
    }

    #[test]
    fn default_with_env_applies_embedding_cache_overrides() {
        let _guard = env_lock().lock().expect("env lock should not be poisoned");
        let old_fastembed = std::env::var(env::app::FASTEMBED_CACHE_DIR).ok();
        let old_snif = std::env::var(env::app::SNIF_EMBEDDING_CACHE_DIR).ok();
        std::env::remove_var(env::app::FASTEMBED_CACHE_DIR);
        std::env::set_var(env::app::SNIF_EMBEDDING_CACHE_DIR, "/tmp/snif-cache");

        let config = SnifConfig::default_with_env();

        assert_eq!(config.index.embedding_cache_dir, "/tmp/snif-cache");

        restore_env_var(env::app::FASTEMBED_CACHE_DIR, old_fastembed);
        restore_env_var(env::app::SNIF_EMBEDDING_CACHE_DIR, old_snif);
    }

    fn restore_env_var(key: &str, old_value: Option<String>) {
        match old_value {
            Some(value) => std::env::set_var(key, value),
            None => std::env::remove_var(key),
        }
    }
}
