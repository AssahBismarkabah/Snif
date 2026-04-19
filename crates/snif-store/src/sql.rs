pub mod tables {
    pub const FILES: &str = "files";
    pub const SYMBOLS: &str = "symbols";
    pub const SUMMARIES: &str = "summaries";
    pub const SUMMARY_EMBEDDINGS: &str = "summary_embeddings";
    pub const IMPORTS: &str = "imports";
    pub const REFS: &str = "refs";
    pub const COCHANGE: &str = "cochange";
    pub const SCHEMA_VERSION: &str = "schema_version";
    pub const FEEDBACK_SIGNALS: &str = "feedback_signals";
    pub const FEEDBACK_EMBEDDINGS: &str = "feedback_embeddings";
}

pub mod columns {
    pub const ID: &str = "id";
    pub const PATH: &str = "path";
    pub const HASH: &str = "hash";
    pub const LANGUAGE: &str = "language";
    pub const INDEXED_AT: &str = "indexed_at";
    pub const SYMBOL_ID: &str = "symbol_id";
    pub const FILE_ID: &str = "file_id";
    pub const NAME: &str = "name";
    pub const KIND: &str = "kind";
    pub const START_LINE: &str = "start_line";
    pub const END_LINE: &str = "end_line";
    pub const SIGNATURE: &str = "signature";
    pub const LEVEL: &str = "level";
    pub const SUMMARY: &str = "summary";
    pub const TOKEN_COUNT: &str = "token_count";
    pub const EMBEDDING: &str = "embedding";
    pub const DISTANCE: &str = "distance";
    pub const SOURCE_PATH: &str = "source_path";
    pub const IMPORTED_NAMES: &str = "imported_names";
    pub const SYMBOL_NAME: &str = "symbol_name";
    pub const LINE: &str = "line";
    pub const CORRELATION: &str = "correlation";
    pub const COMMIT_COUNT: &str = "commit_count";
    pub const FILE_ID_A: &str = "file_id_a";
    pub const FILE_ID_B: &str = "file_id_b";
    pub const VERSION: &str = "version";
    pub const TEAM_ID: &str = "team_id";
    pub const SIGNAL_TYPE: &str = "signal_type";
    pub const FINDING_TEXT: &str = "finding_text";
    pub const FINDING_CATEGORY: &str = "finding_category";
    pub const TIMESTAMP: &str = "timestamp";
    pub const SIGNAL_ID: &str = "signal_id";
}

pub mod pragmas {
    pub const JOURNAL_MODE_WAL: &str = "PRAGMA journal_mode=WAL";
    pub const SYNCHRONOUS_NORMAL: &str = "PRAGMA synchronous=NORMAL";
    pub const FOREIGN_KEYS_OFF: &str = "PRAGMA foreign_keys=OFF";
}

pub mod queries {
    pub const DATETIME_NOW: &str = "datetime('now')";
}
