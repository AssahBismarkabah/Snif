mod batch_parser;
mod prompt;

use anyhow::{bail, Result};
use sha2::{Digest, Sha256};
use snif_config::{
    constants::{model, summarizer},
    env::keys,
    ModelConfig,
};
use snif_execution::{is_rate_limit_error, is_reducible_provider_error, LlmClient, LlmRetryPolicy};
use snif_store::Store;
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Semaphore;

pub struct SummarizeStats {
    pub symbols_summarized: usize,
    pub files_summarized: usize,
    pub symbols_skipped_unchanged: usize,
    pub files_skipped_unchanged: usize,
    pub errors: usize,
    pub rate_limited: bool,
    pub provider_limited: bool,
    pub total_duration: Duration,
}

/// Compute a SHA-256 content hash for a symbol's source body.
/// Used to detect whether a summary is stale (the underlying code has changed
/// since the summary was generated).
fn content_hash(body: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(body.as_bytes());
    format!("{:x}", hasher.finalize())
}

#[derive(Clone)]
struct PendingSymbolSummary {
    symbol_id: i64,
    symbol_name: String,
    file_id: i64,
    kind: String,
    body: String,
    user_prompt: String,
    content_hash: String,
}

#[derive(Clone)]
struct PendingFileSummary {
    file_id: i64,
    user_prompt: String,
    content_hash: String,
}

pub fn summarize_all(
    store: &Store,
    repo_root: &Path,
    config: &ModelConfig,
    concurrency: usize,
) -> Result<SummarizeStats> {
    let api_key = std::env::var(keys::SNIF_API_KEY)
        .or_else(|_| std::env::var(keys::OPENAI_API_KEY))
        .unwrap_or_default();

    if api_key.is_empty() {
        tracing::warn!(
            "No API key found ({} or {}). Skipping summarization.",
            keys::SNIF_API_KEY,
            keys::OPENAI_API_KEY
        );
        return Ok(SummarizeStats {
            symbols_summarized: 0,
            files_summarized: 0,
            symbols_skipped_unchanged: 0,
            files_skipped_unchanged: 0,
            errors: 0,
            rate_limited: false,
            provider_limited: false,
            total_duration: Duration::ZERO,
        });
    }

    if config.endpoint.is_empty() || config.summary_model.is_empty() {
        tracing::warn!("No endpoint or summary_model configured. Skipping summarization.");
        return Ok(SummarizeStats {
            symbols_summarized: 0,
            files_summarized: 0,
            symbols_skipped_unchanged: 0,
            files_skipped_unchanged: 0,
            errors: 0,
            rate_limited: false,
            provider_limited: false,
            total_duration: Duration::ZERO,
        });
    }

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    rt.block_on(summarize_all_async(
        store,
        repo_root,
        config,
        &api_key,
        concurrency,
    ))
}

async fn summarize_all_async(
    store: &Store,
    repo_root: &Path,
    config: &ModelConfig,
    api_key: &str,
    concurrency: usize,
) -> Result<SummarizeStats> {
    let symbols = store.get_symbols_for_summarization()?;
    tracing::info!(
        symbols = symbols.len(),
        concurrency,
        "Starting summarization"
    );

    summarize_symbols_async(store, repo_root, config, api_key, &symbols, concurrency).await
}

async fn summarize_symbols_async(
    store: &Store,
    repo_root: &Path,
    config: &ModelConfig,
    api_key: &str,
    symbols: &[snif_types::SymbolForSummary],
    concurrency: usize,
) -> Result<SummarizeStats> {
    let start = Instant::now();
    let client = Arc::new(LlmClient::from_config(config, api_key, false));
    let concurrency = normalize_concurrency(concurrency);
    let semaphore = Arc::new(Semaphore::new(concurrency));

    let mut symbols_summarized = 0;
    let mut symbols_skipped_unchanged = 0;
    let mut files_skipped_unchanged = 0;
    let mut errors = 0;
    let mut rate_limited = false;
    let mut provider_limited = false;
    let mut pressure_tracker = ProviderPressureTracker::default();

    // Batch 1: Functions and methods — grouped by file and batched
    let functions: Vec<_> = symbols
        .iter()
        .filter(|s| s.kind == summarizer::KIND_FUNCTION || s.kind == summarizer::KIND_METHOD)
        .collect();

    tracing::info!(count = functions.len(), "Summarizing functions");

    let mut pending_symbols = Vec::new();
    for sym in &functions {
        let body = match read_symbol_body(repo_root, &sym.file_path, sym.start_line, sym.end_line) {
            Ok(b) => b,
            Err(_) => continue,
        };

        let hash = content_hash(&body);

        // Skip if summary exists and content hash matches (symbol is unchanged)
        if let Some((_, _existing_summary, existing_hash)) = store.get_summary_for_symbol(sym.id)? {
            match existing_hash {
                Some(stored_hash) if stored_hash == hash => {
                    symbols_skipped_unchanged += 1;
                    continue;
                }
                Some(_) => {
                    // Hash mismatch — content changed, delete only this symbol's summary
                    tracing::debug!(symbol = %sym.name, "Content hash changed, re-summarizing");
                    store.delete_summary_for_symbol(sym.id)?;
                }
                None => {
                    // Legacy summary without hash — treat as stale, delete only this symbol's summary
                    tracing::debug!(symbol = %sym.name, "Legacy summary without hash, re-summarizing");
                    store.delete_summary_for_symbol(sym.id)?;
                }
            }
            // If we get here, the old summary was deleted; fall through to re-summarize
        }

        // Skip if a summary already exists (may have been created by delete+re-check
        // in the case of hash mismatch above, or genuinely exists without hash)
        if store.get_summary_for_symbol(sym.id)?.is_some() {
            continue;
        }

        let user_prompt = prompt::function_prompt(&sym.file_path, &sym.name, &sym.kind, &body);
        pending_symbols.push(PendingSymbolSummary {
            symbol_id: sym.id,
            symbol_name: sym.name.clone(),
            file_id: sym.file_id,
            kind: sym.kind.clone(),
            user_prompt,
            body,
            content_hash: hash,
        });
    }

    // Group pending symbols by file for batch calls
    let mut by_file: HashMap<i64, Vec<PendingSymbolSummary>> = HashMap::new();
    for sym in &pending_symbols {
        by_file.entry(sym.file_id).or_default().push(sym.clone());
    }

    // Pre-build file_id -> file_path lookup for O(1) access during batching
    let file_path_map: HashMap<i64, &str> = symbols
        .iter()
        .map(|s| (s.file_id, s.file_path.as_str()))
        .collect();

    // Separate single-symbol files (use individual path) from multi-symbol files (batch)
    let mut batch_groups: Vec<Vec<PendingSymbolSummary>> = Vec::new();
    let mut fallback_symbols: Vec<PendingSymbolSummary> = Vec::new();

    for file_symbols in by_file.values() {
        for batch in file_symbols.chunks(model::SUMMARIZER_BATCH_SIZE) {
            if batch.len() == 1 {
                // Single symbol — use individual path (no point in a batch of 1)
                fallback_symbols.push(batch[0].clone());
            } else {
                batch_groups.push(batch.to_vec());
            }
        }
    }

    tracing::info!(
        batches = batch_groups.len(),
        individual = fallback_symbols.len(),
        "Batch summarization plan"
    );

    // Process batch groups concurrently using the same semaphore pattern
    let mut batch_iter = batch_groups.into_iter();
    loop {
        let mut tasks = Vec::new();
        let mut batch_sizes = Vec::new();
        for batch in batch_iter.by_ref().take(concurrency) {
            let batch_len = batch.len();
            let batch_data: Vec<(String, String, String)> = batch
                .iter()
                .map(|s| (s.symbol_name.clone(), s.kind.clone(), s.body.clone()))
                .collect();

            let file_path_str = file_path_map
                .get(&batch[0].file_id)
                .copied()
                .unwrap_or("unknown");

            let prompt = prompt::batch_prompt(file_path_str, &batch_data);
            let name_to_id: HashMap<String, (i64, String)> = batch
                .iter()
                .map(|s| (s.symbol_name.clone(), (s.symbol_id, s.content_hash.clone())))
                .collect();

            batch_sizes.push(batch_len);
            let client = Arc::clone(&client);
            let sem = Arc::clone(&semaphore);

            tasks.push(tokio::spawn(async move {
                let _permit = sem
                    .acquire()
                    .await
                    .expect("semaphore should not be closed during summarization");
                let result = client
                    .chat_completion_with_max_tokens_and_policy(
                        prompt::BATCH_SYSTEM_PROMPT,
                        &prompt,
                        Some(model::SUMMARY_OUTPUT_MAX_TOKENS),
                        LlmRetryPolicy::SurfaceReducibleProviderErrors,
                    )
                    .await;
                (batch, name_to_id, result)
            }));
        }

        if tasks.is_empty() {
            break;
        }

        let mut batch_rate_limit_errors = 0;
        let mut batch_provider_pressure_errors = 0;
        for task in tasks {
            match task.await {
                Ok((batch, name_to_id, Ok(response))) => {
                    let parsed = batch_parser::parse_batch_response(&response);
                    let mut matched_names = std::collections::HashSet::new();

                    for summary in &parsed {
                        if let Some((sym_id, hash)) = name_to_id.get(&summary.symbol_name) {
                            let token_count = (summary.summary.len()
                                / summarizer::TOKEN_ESTIMATION_DIVISOR)
                                as i32;
                            if store
                                .insert_summary(
                                    Some(*sym_id),
                                    None,
                                    summarizer::KIND_FUNCTION,
                                    &summary.summary,
                                    Some(hash),
                                    Some(token_count),
                                )
                                .is_ok()
                            {
                                symbols_summarized += 1;
                                matched_names.insert(summary.symbol_name.clone());
                                tracing::debug!(symbol = %summary.symbol_name, "Summarized via batch");
                            }
                        }
                    }

                    // Collect unmatched symbols for individual fallback
                    for s in batch {
                        if !matched_names.contains(&s.symbol_name) {
                            tracing::debug!(symbol = %s.symbol_name, "Batch response missing symbol, falling back to individual call");
                            fallback_symbols.push(s);
                        }
                    }
                }
                Ok((batch, _, Err(e))) => {
                    if is_rate_limit_error(&e) {
                        batch_rate_limit_errors += 1;
                    }
                    if is_reducible_provider_error(&e) {
                        batch_provider_pressure_errors += batch.len();
                    }
                    tracing::warn!(error = %e, "Batch LLM call failed, falling back to individual calls for all symbols in batch");
                    // Entire batch failed — retry all symbols individually
                    fallback_symbols.extend(batch);
                    errors += 1;
                }
                Err(e) => {
                    tracing::warn!(error = %e, "Batch task panicked");
                    errors += 1;
                }
            }
        }

        let total_in_flight: usize = batch_sizes.iter().sum();
        if let Some(reason) = pressure_tracker.record_batch(
            total_in_flight,
            batch_provider_pressure_errors,
            start.elapsed(),
        ) {
            rate_limited |= batch_rate_limit_errors > 0;
            provider_limited = true;
            tracing::warn!(
                concurrency,
                provider_pressure_errors = batch_provider_pressure_errors,
                total_provider_pressure_errors = pressure_tracker.total_errors,
                reason = reason.as_str(),
                "Stopping batch summarization because provider pressure is sustained"
            );
            break;
        }
    }

    if symbols_skipped_unchanged > 0 {
        tracing::info!(
            skipped_unchanged = symbols_skipped_unchanged,
            "Skipped unchanged symbols (content hash match)"
        );
    }

    // Process fallback symbols individually
    if !fallback_symbols.is_empty() && !provider_limited {
        tracing::info!(
            count = fallback_symbols.len(),
            "Summarizing fallback symbols individually"
        );

        for batch in fallback_symbols.chunks(concurrency) {
            let mut tasks = Vec::new();
            for pending in batch {
                let client = Arc::clone(&client);
                let sem = Arc::clone(&semaphore);
                let pending = pending.clone();

                tasks.push(tokio::spawn(async move {
                    let _permit = sem
                        .acquire()
                        .await
                        .expect("semaphore should not be closed during summarization");
                    let result = client
                        .chat_completion_with_max_tokens_and_policy(
                            prompt::SYSTEM_PROMPT,
                            &pending.user_prompt,
                            Some(model::SUMMARY_OUTPUT_MAX_TOKENS),
                            LlmRetryPolicy::SurfaceReducibleProviderErrors,
                        )
                        .await;
                    (
                        pending.symbol_id,
                        pending.symbol_name,
                        pending.content_hash,
                        result,
                    )
                }));
            }

            let mut batch_rate_limit_errors = 0;
            let mut batch_provider_pressure_errors = 0;
            for task in tasks {
                match task.await {
                    Ok((sym_id, name, hash, Ok(summary))) => {
                        let token_count =
                            (summary.len() / summarizer::TOKEN_ESTIMATION_DIVISOR) as i32;
                        store.insert_summary(
                            Some(sym_id),
                            None,
                            summarizer::KIND_FUNCTION,
                            &summary,
                            Some(&hash),
                            Some(token_count),
                        )?;
                        symbols_summarized += 1;
                        tracing::debug!(symbol = %name, "Summarized individually");
                    }
                    Ok((_, name, _, Err(e))) => {
                        if is_reducible_provider_error(&e) {
                            batch_provider_pressure_errors += 1;
                        }
                        if is_rate_limit_error(&e) {
                            batch_rate_limit_errors += 1;
                        }
                        tracing::warn!(symbol = %name, error = %e, "Failed to summarize");
                        errors += 1;
                    }
                    Err(e) => {
                        tracing::warn!(error = %e, "Task panicked");
                        errors += 1;
                    }
                }
            }

            if let Some(reason) = pressure_tracker.record_batch(
                batch.len(),
                batch_provider_pressure_errors,
                start.elapsed(),
            ) {
                rate_limited |= batch_rate_limit_errors > 0;
                provider_limited = true;
                tracing::warn!(
                    concurrency,
                    provider_pressure_errors = batch_provider_pressure_errors,
                    total_provider_pressure_errors = pressure_tracker.total_errors,
                    reason = reason.as_str(),
                    "Stopping individual fallback summarization because provider pressure is sustained"
                );
                break;
            }
        }
    }

    // Batch 2: File-level summaries
    let file_ids: Vec<i64> = symbols
        .iter()
        .map(|s| s.file_id)
        .collect::<std::collections::HashSet<_>>()
        .into_iter()
        .collect();

    // Build a map of file_id -> path
    let file_paths: HashMap<i64, String> = symbols
        .iter()
        .map(|s| (s.file_id, s.file_path.clone()))
        .collect();

    let file_stats = summarize_file_levels(
        store,
        &client,
        &semaphore,
        concurrency,
        &file_ids,
        &file_paths,
    )
    .await?;

    let files_summarized = file_stats.files_summarized;
    files_skipped_unchanged += file_stats.files_skipped_unchanged;
    errors += file_stats.errors;
    rate_limited |= file_stats.rate_limited;
    provider_limited |= file_stats.provider_limited;

    Ok(SummarizeStats {
        symbols_summarized,
        files_summarized,
        symbols_skipped_unchanged,
        files_skipped_unchanged,
        errors,
        rate_limited,
        provider_limited,
        total_duration: start.elapsed(),
    })
}

/// Generate file-level summaries from child symbol summaries.
///
/// For each file, computes a content hash from its child symbol summaries,
/// checks if the file-level summary is stale, and if so generates a new one.
/// Uses the provided client and semaphore for concurrent LLM calls.
struct FileLevelStats {
    files_summarized: usize,
    files_skipped_unchanged: usize,
    errors: usize,
    rate_limited: bool,
    provider_limited: bool,
}

async fn summarize_file_levels(
    store: &Store,
    client: &Arc<LlmClient>,
    semaphore: &Arc<Semaphore>,
    concurrency: usize,
    file_ids: &[i64],
    file_paths: &HashMap<i64, String>,
) -> Result<FileLevelStats> {
    let mut files_summarized = 0;
    let mut files_skipped_unchanged = 0;
    let mut errors = 0;
    let mut rate_limited = false;
    let mut provider_limited = false;
    let mut pressure_tracker = ProviderPressureTracker::default();
    let batch_start = Instant::now();

    tracing::info!(count = file_ids.len(), "Summarizing files");

    let mut pending_files = Vec::new();
    let mut files_skipped = 0;
    for file_id in file_ids {
        let child_summaries = store.get_summaries_for_file_symbols(*file_id)?;
        if child_summaries.is_empty() {
            continue;
        }

        // Compute content hash from child summaries
        let children: Vec<(String, String)> = child_summaries
            .iter()
            .map(|(_, name, _, summary)| (name.clone(), summary.clone()))
            .collect();
        let file_hash = content_hash(
            &children
                .iter()
                .map(|(n, s)| format!("{}:{}", n, s))
                .collect::<Vec<_>>()
                .join("\n"),
        );

        // Skip if file-level summary exists and content hash matches
        if let Some((_, _)) = store.get_summary_for_file(*file_id)? {
            match store.get_file_content_hash(*file_id)? {
                Some(stored_hash) if stored_hash == file_hash => {
                    files_skipped_unchanged += 1;
                    continue;
                }
                Some(_) => {
                    // Hash mismatch — child summaries changed, delete file-level summary only
                    tracing::debug!(file_id, "File content hash changed, re-summarizing");
                    store.delete_file_level_summary(*file_id)?;
                }
                None => {
                    // Legacy summary without hash — treat as stale
                    tracing::debug!(file_id, "Legacy file summary without hash, re-summarizing");
                    store.delete_file_level_summary(*file_id)?;
                }
            }
            // After deletion, check again in case deletion failed
            if store.get_summary_for_file(*file_id)?.is_some() {
                files_skipped += 1;
                continue;
            }
        }

        // Skip if file-level summary already exists from a prior run
        if store.get_summary_for_file(*file_id)?.is_some() {
            files_skipped += 1;
            continue;
        }

        let file_path = match file_paths.get(file_id) {
            Some(p) => p.clone(),
            None => continue,
        };

        pending_files.push(PendingFileSummary {
            file_id: *file_id,
            user_prompt: prompt::file_prompt(&file_path, &children),
            content_hash: file_hash,
        });
    }

    if files_skipped > 0 {
        tracing::info!(skipped = files_skipped, "Skipped already-summarized files");
    }
    if files_skipped_unchanged > 0 {
        tracing::info!(
            skipped_unchanged = files_skipped_unchanged,
            "Skipped unchanged files (content hash match)"
        );
    }

    for batch in pending_files.chunks(concurrency) {
        let mut tasks = Vec::new();
        for pending in batch {
            let client = Arc::clone(client);
            let sem = Arc::clone(semaphore);
            let pending = pending.clone();

            tasks.push(tokio::spawn(async move {
                let _permit = sem
                    .acquire()
                    .await
                    .expect("semaphore should not be closed during file summarization");
                let result = client
                    .chat_completion_with_max_tokens_and_policy(
                        prompt::SYSTEM_PROMPT,
                        &pending.user_prompt,
                        Some(model::SUMMARY_OUTPUT_MAX_TOKENS),
                        LlmRetryPolicy::SurfaceReducibleProviderErrors,
                    )
                    .await;
                (pending.file_id, pending.content_hash, result)
            }));
        }

        let mut batch_rate_limit_errors = 0;
        let mut batch_provider_pressure_errors = 0;
        for task in tasks {
            match task.await {
                Ok((file_id, hash, Ok(summary))) => {
                    let token_count = (summary.len() / summarizer::TOKEN_ESTIMATION_DIVISOR) as i32;
                    store.insert_summary(
                        None,
                        Some(file_id),
                        summarizer::LEVEL_FILE,
                        &summary,
                        Some(&hash),
                        Some(token_count),
                    )?;
                    files_summarized += 1;
                }
                Ok((file_id, _, Err(e))) => {
                    if is_reducible_provider_error(&e) {
                        batch_provider_pressure_errors += 1;
                    }
                    if is_rate_limit_error(&e) {
                        batch_rate_limit_errors += 1;
                    }
                    tracing::warn!(file_id, error = %e, "Failed to summarize file");
                    errors += 1;
                }
                Err(e) => {
                    tracing::warn!(error = %e, "File task panicked");
                    errors += 1;
                }
            }
        }

        if let Some(reason) = pressure_tracker.record_batch(
            batch.len(),
            batch_provider_pressure_errors,
            batch_start.elapsed(),
        ) {
            rate_limited |= batch_rate_limit_errors > 0;
            provider_limited = true;
            tracing::warn!(
                concurrency,
                provider_pressure_errors = batch_provider_pressure_errors,
                total_provider_pressure_errors = pressure_tracker.total_errors,
                reason = reason.as_str(),
                "Stopping file summarization because provider pressure is sustained"
            );
            break;
        }
    }

    Ok(FileLevelStats {
        files_summarized,
        files_skipped_unchanged,
        errors,
        rate_limited,
        provider_limited,
    })
}

/// Summarize files on demand. Given a list of file paths, looks up their symbols
/// in the store and generates summaries for any that don't already have one.
/// This is the entry point for lazy/on-demand summarization during review.
///
/// Only processes symbols belonging to the given file paths, not the entire repo.
/// Returns stats about what was summarized.
pub fn summarize_files(
    store: &Store,
    repo_root: &Path,
    config: &ModelConfig,
    file_paths: &[String],
    concurrency: usize,
) -> Result<SummarizeStats> {
    let api_key = std::env::var(keys::SNIF_API_KEY)
        .or_else(|_| std::env::var(keys::OPENAI_API_KEY))
        .unwrap_or_default();

    if api_key.is_empty() {
        tracing::warn!("No API key found for on-demand summarization. Skipping.");
        return Ok(SummarizeStats {
            symbols_summarized: 0,
            files_summarized: 0,
            symbols_skipped_unchanged: 0,
            files_skipped_unchanged: 0,
            errors: 0,
            rate_limited: false,
            provider_limited: false,
            total_duration: Duration::ZERO,
        });
    }

    if config.endpoint.is_empty() || config.summary_model.is_empty() {
        tracing::warn!(
            "No endpoint or summary_model configured. Skipping on-demand summarization."
        );
        return Ok(SummarizeStats {
            symbols_summarized: 0,
            files_summarized: 0,
            symbols_skipped_unchanged: 0,
            files_skipped_unchanged: 0,
            errors: 0,
            rate_limited: false,
            provider_limited: false,
            total_duration: Duration::ZERO,
        });
    }

    let start = Instant::now();

    // Resolve file paths to file IDs
    let file_ids = store.get_file_ids_batch(file_paths)?;
    if file_ids.is_empty() {
        tracing::debug!("No indexed files found for on-demand summarization");
        return Ok(SummarizeStats {
            symbols_summarized: 0,
            files_summarized: 0,
            symbols_skipped_unchanged: 0,
            files_skipped_unchanged: 0,
            errors: 0,
            rate_limited: false,
            provider_limited: false,
            total_duration: start.elapsed(),
        });
    }

    let just_file_ids: Vec<i64> = file_ids.iter().map(|(id, _)| *id).collect();

    // Fetch only the symbols for the requested files
    let symbols = store.get_symbols_for_files(&just_file_ids)?;
    tracing::info!(
        files = file_ids.len(),
        symbols = symbols.len(),
        "On-demand summarization starting"
    );

    if symbols.is_empty() {
        return Ok(SummarizeStats {
            symbols_summarized: 0,
            files_summarized: 0,
            symbols_skipped_unchanged: 0,
            files_skipped_unchanged: 0,
            errors: 0,
            rate_limited: false,
            provider_limited: false,
            total_duration: start.elapsed(),
        });
    }

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    rt.block_on(summarize_symbols_async(
        store,
        repo_root,
        config,
        &api_key,
        &symbols,
        concurrency,
    ))
}

fn normalize_concurrency(concurrency: usize) -> usize {
    concurrency.max(1)
}

const PROVIDER_PRESSURE_TOTAL_ERROR_LIMIT: usize = 10;
const PROVIDER_PRESSURE_CONSECUTIVE_BATCH_LIMIT: usize = 2;
const PROVIDER_PRESSURE_TIME_LIMIT: Duration = Duration::from_secs(600);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ProviderPressureStopReason {
    FullBatch,
    RepeatedPartialBatches,
    TotalErrors,
    TimeBudget,
}

impl ProviderPressureStopReason {
    fn as_str(self) -> &'static str {
        match self {
            Self::FullBatch => "full_batch",
            Self::RepeatedPartialBatches => "repeated_partial_batches",
            Self::TotalErrors => "total_errors",
            Self::TimeBudget => "time_budget",
        }
    }
}

#[derive(Debug, Default)]
struct ProviderPressureTracker {
    total_errors: usize,
    consecutive_high_pressure_batches: usize,
    saw_pressure: bool,
}

impl ProviderPressureTracker {
    fn record_batch(
        &mut self,
        batch_len: usize,
        provider_pressure_errors: usize,
        elapsed: Duration,
    ) -> Option<ProviderPressureStopReason> {
        if provider_pressure_errors > 0 {
            self.saw_pressure = true;
            self.total_errors += provider_pressure_errors;
        }

        if self.saw_pressure && elapsed >= PROVIDER_PRESSURE_TIME_LIMIT {
            return Some(ProviderPressureStopReason::TimeBudget);
        }

        if batch_len == 0 || provider_pressure_errors == 0 {
            self.consecutive_high_pressure_batches = 0;
            return None;
        }

        if provider_pressure_errors == batch_len {
            return Some(ProviderPressureStopReason::FullBatch);
        }

        if provider_pressure_errors * 2 >= batch_len {
            self.consecutive_high_pressure_batches += 1;
        } else {
            self.consecutive_high_pressure_batches = 0;
        }

        if self.consecutive_high_pressure_batches >= PROVIDER_PRESSURE_CONSECUTIVE_BATCH_LIMIT {
            return Some(ProviderPressureStopReason::RepeatedPartialBatches);
        }

        if self.total_errors >= PROVIDER_PRESSURE_TOTAL_ERROR_LIMIT {
            return Some(ProviderPressureStopReason::TotalErrors);
        }

        None
    }
}

fn read_symbol_body(
    repo_root: &Path,
    file_path: &str,
    start_line: i64,
    end_line: i64,
) -> Result<String> {
    let full_path = repo_root.join(file_path);
    let content = std::fs::read_to_string(&full_path)?;
    let lines: Vec<&str> = content.lines().collect();

    let start = (start_line as usize).saturating_sub(1);
    let end = (end_line as usize).min(lines.len());

    if start >= lines.len() {
        bail!("Start line {} out of range for {}", start_line, file_path);
    }

    Ok(lines[start..end].join("\n"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zero_concurrency_is_clamped_to_one() {
        assert_eq!(normalize_concurrency(0), 1);
        assert_eq!(normalize_concurrency(3), 3);
    }

    #[test]
    fn full_provider_pressure_batch_stops_summarization() {
        let mut tracker = ProviderPressureTracker::default();

        assert_eq!(
            tracker.record_batch(3, 3, Duration::from_secs(1)),
            Some(ProviderPressureStopReason::FullBatch)
        );
    }

    #[test]
    fn single_partial_provider_pressure_batch_does_not_stop_summarization() {
        let mut tracker = ProviderPressureTracker::default();

        assert_eq!(tracker.record_batch(3, 1, Duration::from_secs(1)), None);
        assert_eq!(tracker.record_batch(3, 0, Duration::from_secs(2)), None);
        assert_eq!(tracker.record_batch(0, 0, Duration::from_secs(3)), None);
    }

    #[test]
    fn repeated_partial_provider_pressure_batches_stop_summarization() {
        let mut tracker = ProviderPressureTracker::default();

        assert_eq!(tracker.record_batch(4, 2, Duration::from_secs(1)), None);
        assert_eq!(
            tracker.record_batch(4, 2, Duration::from_secs(2)),
            Some(ProviderPressureStopReason::RepeatedPartialBatches)
        );
    }

    #[test]
    fn total_provider_pressure_errors_stop_summarization() {
        let mut tracker = ProviderPressureTracker::default();

        assert_eq!(tracker.record_batch(20, 4, Duration::from_secs(1)), None);
        assert_eq!(tracker.record_batch(20, 4, Duration::from_secs(2)), None);
        assert_eq!(
            tracker.record_batch(20, 2, Duration::from_secs(3)),
            Some(ProviderPressureStopReason::TotalErrors)
        );
    }

    #[test]
    fn provider_pressure_time_budget_only_applies_after_pressure() {
        let mut tracker = ProviderPressureTracker::default();

        assert_eq!(tracker.record_batch(4, 0, Duration::from_secs(600)), None);
        assert_eq!(tracker.record_batch(4, 1, Duration::from_secs(10)), None);
        assert_eq!(
            tracker.record_batch(4, 0, Duration::from_secs(600)),
            Some(ProviderPressureStopReason::TimeBudget)
        );
    }
}
