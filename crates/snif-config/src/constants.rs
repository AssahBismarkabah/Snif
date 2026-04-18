pub mod model {
    pub const DEFAULT_EMBEDDING_DIMENSION: usize = 384;
    pub const DEFAULT_MAX_TOKENS: usize = 128_000;
    pub const DEFAULT_OUTPUT_RESERVE_TOKENS: usize = 32_000;
    pub const DEFAULT_MAX_FILES: usize = 50;
    pub const MAX_CONCURRENT_SUMMARIZATION: usize = 3;
    pub const EMBEDDING_BATCH_SIZE: usize = 64;
}

pub mod retrieval {
    pub const SEMANTIC_KNN_K: usize = 20;
    pub const MIN_COCHANGE_CORRELATION: f64 = 0.1;
    pub const MIN_COCHANGE_RETRIEVAL_CORRELATION: f64 = 0.2;
    pub const MAX_FILES_PER_COMMIT: usize = 50;
    pub const DIRECT_IMPORT_SCORE: f64 = 1.0;
    pub const REVERSE_IMPORT_SCORE: f64 = 0.8;
    pub const SYMBOL_REFERENCE_SCORE: f64 = 0.6;
    pub const SEMANTIC_SIMILARITY_FLOOR: f64 = 0.0;
    pub const MAX_KEYWORD_TERMS: usize = 3;
}

pub mod limits {
    pub const MAX_FILE_SIZE_BYTES: usize = 1_000_000;
    pub const TEXT_DETECTION_SAMPLE_SIZE: usize = 512;
    pub const MAX_CHANGED_FILE_BYTES: usize = 50_000;
    pub const MAX_SUMMARIES_FETCH_LIMIT: usize = 50_000;
    pub const MAX_SYMBOLS_FETCH_LIMIT: usize = 10_000;
}

pub mod timeouts {
    pub const LLM_REQUEST_TIMEOUT_SECS: u64 = 300;
    pub const LLM_MAX_RETRIES: u32 = 5;
    pub const LLM_RETRY_BASE_DELAY_SECS: u64 = 2;
    pub const HTTP_TIMEOUT_SECS: u64 = 15;
    pub const JWT_CLOCK_DRIFT_SECS: u64 = 60;
    pub const JWT_EXPIRY_SECS: u64 = 600;
    pub const GITLAB_MAX_PAGES: usize = 100;
    pub const GITLAB_PER_PAGE: usize = 100;
}

pub mod thresholds {
    pub const SARIF_ERROR_CONFIDENCE: f64 = 0.9;
    pub const SARIF_WARNING_CONFIDENCE: f64 = 0.7;
    pub const MIN_CONFIDENCE_DEFAULT: f64 = 0.7;
    pub const FEEDBACK_MIN_SIGNALS: usize = 20;
}

pub mod time {
    pub const SECS_PER_MINUTE: u64 = 60;
    pub const SECS_PER_HOUR: u64 = 3600;
    pub const SECS_PER_DAY: u64 = 86400;
}
