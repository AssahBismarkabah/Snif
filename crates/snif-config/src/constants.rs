// ============================================================================
// Snif Configuration Constants
//
// Centralized constants for magic numbers, thresholds, limits, timeouts,
// prompt templates, CLI defaults, and output formatting.
// ============================================================================

// ============================================================================
// LLM Model Configuration
// ============================================================================
pub mod model {
    /// Default dimension for embedding vectors (all-MiniLM-L6-v2)
    pub const DEFAULT_EMBEDDING_DIMENSION: usize = 384;
    /// Maximum context window for LLM requests
    pub const DEFAULT_MAX_TOKENS: usize = 128_000;
    /// Reserved tokens for LLM output generation
    pub const DEFAULT_OUTPUT_RESERVE_TOKENS: usize = 32_000;
    /// Maximum number of files to include in review context
    pub const DEFAULT_MAX_FILES: usize = 50;
    /// Max concurrent summarization tasks
    pub const MAX_CONCURRENT_SUMMARIZATION: usize = 3;
    /// Batch size for embedding API calls
    pub const EMBEDDING_BATCH_SIZE: usize = 64;
}

// ============================================================================
// Code Retrieval Configuration
// ============================================================================
pub mod retrieval {
    /// K value for KNN semantic search
    pub const SEMANTIC_KNN_K: usize = 20;
    /// Minimum correlation for co-change analysis (low threshold)
    pub const MIN_COCHANGE_CORRELATION: f64 = 0.1;
    /// Minimum correlation for co-change retrieval (higher threshold)
    pub const MIN_COCHANGE_RETRIEVAL_CORRELATION: f64 = 0.2;
    /// Maximum files per commit for co-change analysis
    pub const MAX_FILES_PER_COMMIT: usize = 50;
    /// Base score for direct imports in structural retrieval
    pub const DIRECT_IMPORT_SCORE: f64 = 1.0;
    /// Score for reverse imports in structural retrieval
    pub const REVERSE_IMPORT_SCORE: f64 = 0.8;
    /// Score for symbol references in structural retrieval
    pub const SYMBOL_REFERENCE_SCORE: f64 = 0.6;
    /// Floor for semantic similarity scoring
    pub const SEMANTIC_SIMILARITY_FLOOR: f64 = 0.0;
    /// Max keyword terms counted for retrieval scoring
    pub const MAX_KEYWORD_TERMS: usize = 3;
}

// ============================================================================
// Size and Resource Limits
// ============================================================================
pub mod limits {
    /// Maximum file size for parsing (1 MB)
    pub const MAX_FILE_SIZE_BYTES: usize = 1_000_000;
    /// Sample size for text heuristic detection (512 bytes)
    pub const TEXT_DETECTION_SAMPLE_SIZE: usize = 512;
    /// Maximum bytes for changed file content inclusion
    pub const MAX_CHANGED_FILE_BYTES: usize = 50_000;
    /// Pagination limit for summary fetching
    pub const MAX_SUMMARIES_FETCH_LIMIT: usize = 50_000;
    /// Pagination limit for symbol fetching
    pub const MAX_SYMBOLS_FETCH_LIMIT: usize = 10_000;
}

// ============================================================================
// Timeout and Retry Configuration
// ============================================================================
pub mod timeouts {
    /// LLM request timeout in seconds (5 minutes)
    pub const LLM_REQUEST_TIMEOUT_SECS: u64 = 300;
    /// Maximum retry attempts for LLM requests
    pub const LLM_MAX_RETRIES: u32 = 5;
    /// Base delay for exponential backoff (2 seconds)
    pub const LLM_RETRY_BASE_DELAY_SECS: u64 = 2;
    /// Generic HTTP client timeout in seconds
    pub const HTTP_TIMEOUT_SECS: u64 = 15;
    /// Clock drift tolerance for JWT tokens (60 seconds)
    pub const JWT_CLOCK_DRIFT_SECS: u64 = 60;
    /// JWT token expiry duration (10 minutes)
    pub const JWT_EXPIRY_SECS: u64 = 600;
    /// Maximum pages for GitLab API pagination
    pub const GITLAB_MAX_PAGES: usize = 100;
    /// Items per page for GitLab API requests
    pub const GITLAB_PER_PAGE: usize = 100;
}

// ============================================================================
// Confidence Thresholds
// ============================================================================
pub mod thresholds {
    /// Confidence threshold for SARIF error-level findings
    pub const SARIF_ERROR_CONFIDENCE: f64 = 0.9;
    /// Confidence threshold for SARIF warning-level findings
    pub const SARIF_WARNING_CONFIDENCE: f64 = 0.7;
    /// Default minimum confidence for finding inclusion
    pub const MIN_CONFIDENCE_DEFAULT: f64 = 0.7;
    /// Minimum signals for feedback analysis
    pub const FEEDBACK_MIN_SIGNALS: usize = 20;
    /// Precision drop threshold for regression detection
    pub const PRECISION_REGRESSION_THRESHOLD: f64 = 0.05;
    /// Recall drop threshold for regression detection
    pub const RECALL_REGRESSION_THRESHOLD: f64 = 0.10;
    /// Noise increase threshold for regression detection
    pub const NOISE_REGRESSION_THRESHOLD: f64 = 0.05;
}

// ============================================================================
// LLM Prompt Templates
// ============================================================================
pub mod prompts {
    // System prompt sections
    pub const SYSTEM_PROMPT_INTRO: &str =
        "You are a strict, precision-focused code reviewer. Your job is to find real issues \
         in code changes — bugs, security vulnerabilities, logic errors, and convention \
         violations that have concrete impact.";

    pub const SYSTEM_PROMPT_JSON_INSTRUCTION: &str =
        "Return ONLY one valid JSON object. Do not output markdown fences, analysis, \
         step-by-step reasoning, or any text before or after the JSON. Your first character \
         must be '{' and your last character must be '}'.";

    pub const SYSTEM_PROMPT_RULES: &[&str] = &[
        "- Bias toward false negatives over false positives. If you are not confident, stay quiet.",
        "- Keep reasoning internal. Never expose chain-of-thought.",
        "- Do NOT include a finding if you are uncertain or conclude there is no real issue.",
        "- A finding is only valid when you are confident it represents a concrete problem \
          with specific user-visible impact. Uncertainty = empty findings array.",
        "- Never include phrases like \"no bug\", \"no issue\", \"acceptable\", \"I will\", \
          or reasoning narration in the explanation or impact fields.",
        "- If you start analyzing something and decide it is not a bug, omit it entirely. \
          Do NOT include a finding whose purpose is to explain why there is no bug.",
        "- Every finding MUST cite specific evidence from the provided code.",
        "- Every finding MUST explain the user-relevant impact — what breaks, what is at risk.",
        "- Do NOT flag speculative or hypothetical issues.",
        "- Do NOT flag issues you cannot ground in the provided context.",
        "- Prefer one finding per distinct root cause. Do not split one underlying bug into \
          multiple overlapping findings.",
        "- Do NOT flag micro-optimizations (unnecessary allocations, format patterns, iterator \
          vs collect, clone vs borrow) unless the code is in a measured hot path or processes \
          unbounded input. Focus on bugs that break correctness or security.",
        "- Treat database queries inside loops as real performance bugs.",
        "- Treat reading or collecting unbounded user-controlled input into memory without \
          size limits as a real performance bug.",
        "- Treat joining user-controlled path segments onto a base directory without validation \
          or normalization as a security bug.",
        "- Treat recursive or generic merges of user-controlled objects into plain objects \
          without blocking \"__proto__\", \"prototype\", or \"constructor\" keys as a \
          security bug (prototype pollution).",
        "- If full file content is not provided for a changed file, use the diff hunks to review \
          that file's changes.",
    ];

    pub const SYSTEM_PROMPT_STYLE_SUPPRESSION: &str =
        "- Do NOT flag style-only issues (formatting, naming preferences) \
         unless they violate an explicit project convention.";

    pub const SYSTEM_PROMPT_CONVENTION_INSTRUCTION: &str = "\n\n## Project Conventions\n";

    pub const SYSTEM_PROMPT_CONVENTION_FOOTER: &str =
        "\n\nFlag violations of these conventions with category \"convention\".\n";

    pub const SYSTEM_PROMPT_RESPONSE_FORMAT: &str = "\
\nRespond with a JSON object containing two fields:

1. \"summary\": A 1-2 sentence walkthrough of what this change does and why. \
   Describe the intent and impact on the codebase, not the individual files.

2. \"findings\": A JSON array of issues found. If the change is clean, \
   use an empty array. If you are unsure about the format, return \
   {\"summary\":\"\",\"findings\":[]} exactly.

Line numbers MUST refer to the line numbers in the file content \
provided in the Changed Files section. If file content is omitted, \
use the line numbers from the diff hunks.

Response format:
{
  \"summary\": \"<1-2 sentence walkthrough of the change>\",
  \"findings\": [
    {
      \"file\": \"path/to/file\",
      \"start_line\": <line number in the file>,
      \"end_line\": <line number in the file or null>,
      \"category\": \"logic\" | \"security\" | \"convention\" | \"performance\" | \"style\" | \"other\",
      \"confidence\": <0.0 to 1.0>,
      \"evidence\": \"<quoted code from the diff or context>\",
      \"explanation\": \"<what is wrong and why>\",
      \"impact\": \"<what happens if this is not fixed>\",
      \"suggestion\": \"<optional fix suggestion or null>\"
    }
  ]
}
";

    // User prompt sections
    pub const USER_PROMPT_DIFF_HEADER: &str = "\n\n## Diff\n\n```diff\n";
    pub const USER_PROMPT_DIFF_FOOTER: &str = "\n```\n\n";
    pub const USER_PROMPT_CHANGED_FILES_HEADER: &str = "\n\n## Changed Files\n\n";
    pub const USER_PROMPT_RELATED_FILES_HEADER: &str = "\n\n## Related Files (for context)\n\n";
    pub const USER_PROMPT_LINE_FORMAT: &str = "{:>4} | {}\n";
    pub const USER_PROMPT_DIFF_ONLY_CONTENT: &str = "*{}*\n\n";
    pub const USER_PROMPT_RELEVANCE_FORMAT: &str = " (relevance: {:.2})";
    pub const USER_PROMPT_FINAL_INSTRUCTION: &str = "\
\nReview the diff above. Return only the JSON object described in the system prompt. \
Do not include markdown fences, analysis, or any extra text. Your first character \
must be '{' and your last character must be '}'. If you are unsure, return \
{\"summary\":\"\",\"findings\":[]} exactly.
";

    // Metadata labels
    pub const METADATA_CHANGE_LABEL: &str = "Change: {}\n";
    pub const METADATA_AUTHOR_LABEL: &str = "Author: {}\n";
    pub const METADATA_BRANCH_LABEL: &str = "Base branch: {}\n";
    pub const METADATA_DESCRIPTION_HEADER: &str = "\n\nDescription:\n{}\n";
    pub const METADATA_LABELS_HEADER: &str = "\n\nLabels: {}\n";
    pub const METADATA_COMMITS_HEADER: &str = "\n\nCommits:\n";
    pub const METADATA_COMMIT_ITEM: &str = "- {}\n";
    pub const METADATA_FILE_HEADER: &str = "### {}\n";
    pub const METADATA_SUMMARY_LABEL: &str = "Summary: {}\n";
}

// ============================================================================
// CLI Defaults and Configuration
// ============================================================================
pub mod context {
    /// Files that should not be included in code review context
    pub const NON_REVIEWABLE_FILES: &[&str] = &[
        "pnpm-lock.yaml",
        "package-lock.json",
        "yarn.lock",
        "Cargo.lock",
        "Gemfile.lock",
        "poetry.lock",
        "composer.lock",
        "go.sum",
        "flake.lock",
    ];

    /// Non-reviewable file extension patterns
    pub const NON_REVIEWABLE_EXTENSIONS: &[&str] = &[".lock", ".min.js", ".min.css", ".bundle.js"];

    /// Placeholder text for excluded file content
    pub const CONTENT_EXCLUDED_PLACEHOLDER: &str =
        "[File content excluded — large or generated file. See diff for changes.]";

    /// Placeholder text when content is degraded to diff-only tier
    pub const CONTENT_DIFF_ONLY_PLACEHOLDER: &str = "[See diff for changes to this file.]";

    /// Template for summary-only content with omitted message
    pub const SUMMARY_ONLY_CONTENT_FORMAT: &str = "[Summary — full content omitted.]\n{}";

    /// Omission reason codes for tracking why content was excluded
    pub const REASON_CONTENT_DEGRADED_TO_SUMMARY: &str = "content_degraded_to_summary";
    pub const REASON_CONTENT_DEGRADED_TO_DIFF_ONLY: &str = "content_degraded_to_diff_only";
    pub const REASON_MAX_FILES_EXCEEDED: &str = "max_files_exceeded";
    pub const REASON_TOKEN_BUDGET_EXCEEDED: &str = "token_budget_exceeded";

    /// Token estimation: conservative ratio of characters per token for code
    pub const TOKENS_PER_CHAR_RATIO: usize = 3;
}

pub mod cli {
    pub const DEFAULT_PATH: &str = ".";
    pub const DEFAULT_OUTPUT_FORMAT: &str = "json";
    pub const DEFAULT_EVAL_HISTORY: &str = "eval-history.jsonl";
    pub const OUTPUT_FORMAT_SARIF: &str = "sarif";
    pub const PLATFORM_GITLAB: &str = "gitlab";
    pub const PLATFORM_GITHUB: &str = "github";
    pub const CONTENT_DIFF_ONLY_PLACEHOLDER: &str = "[See diff for changes to this file.]";

    // Error messages
    pub const GITLAB_PROJECT_PATH_REQUIRED: &str =
        "--project or $CI_PROJECT_PATH required for GitLab. \
         Make sure the pipeline runs with: rules: - if: $CI_PIPELINE_SOURCE == \"merge_request_event\"";
    pub const GITLAB_MR_IID_REQUIRED: &str =
        "--pr/--mr or $CI_MERGE_REQUEST_IID required for GitLab. \
         $CI_MERGE_REQUEST_IID is only available in merge request pipelines. \
         Add this rule to your .gitlab-ci.yml: rules: - if: $CI_PIPELINE_SOURCE == \"merge_request_event\"";
    pub const GITHUB_REPOSITORY_REQUIRED: &str = "--repo or GITHUB_REPOSITORY required for GitHub";
    pub const SNIF_PR_NUMBER_REQUIRED: &str = "--pr or SNIF_PR_NUMBER required for GitHub";
    pub const REPO_FORMAT_ERROR: &str = "--repo must be in owner/repo format";

    // CI pipeline references
    pub const CI_PIPELINE_SOURCE_MR_EVENT: &str = "merge_request_event";
    pub const GITLAB_CI_RULES_TEMPLATE: &str =
        "rules: - if: $CI_PIPELINE_SOURCE == \"merge_request_event\"";
}

// ============================================================================
// Clean Command Output
// ============================================================================
pub mod clean {
    /// Directories targeted for removal during clean operation
    pub const CLEAN_TARGETS: &[&str] = &[".snif", ".fastembed_cache"];
    /// Message displayed after successful clean
    pub const CLEAN_COMPLETE_MESSAGE: &str =
        "\n  Clean complete. Configuration (.snif.json) was not touched.";
    /// Message displayed when no targets were found to clean
    pub const CLEAN_NOTHING_TO_CLEAN: &str = "  Nothing to clean.";
    /// Prefix for removed directory messages
    pub const CLEAN_REMOVED_PREFIX: &str = "  Removed ";
}

// ============================================================================
// Time Unit Constants
// ============================================================================
pub mod time {
    pub const SECS_PER_MINUTE: u64 = 60;
    pub const SECS_PER_HOUR: u64 = 3_600;
    pub const SECS_PER_DAY: u64 = 86_400;
}
