mod prompt;

use anyhow::{bail, Result};
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
    pub errors: usize,
    pub rate_limited: bool,
    pub provider_limited: bool,
    pub total_duration: Duration,
}

#[derive(Clone)]
struct PendingSymbolSummary {
    symbol_id: i64,
    symbol_name: String,
    user_prompt: String,
}

#[derive(Clone)]
struct PendingFileSummary {
    file_id: i64,
    user_prompt: String,
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
    let start = Instant::now();
    let client = Arc::new(LlmClient::from_config(config, api_key, false));
    let concurrency = normalize_concurrency(concurrency);
    let semaphore = Arc::new(Semaphore::new(concurrency));

    let symbols = store.get_symbols_for_summarization()?;
    tracing::info!(
        symbols = symbols.len(),
        concurrency,
        "Starting summarization"
    );

    let mut symbols_summarized = 0;
    let mut files_summarized = 0;
    let mut errors = 0;
    let mut rate_limited = false;
    let mut provider_limited = false;
    let mut pressure_tracker = ProviderPressureTracker::default();

    // Batch 1: Functions and methods
    let functions: Vec<_> = symbols
        .iter()
        .filter(|s| s.kind == summarizer::KIND_FUNCTION || s.kind == summarizer::KIND_METHOD)
        .collect();

    tracing::info!(count = functions.len(), "Summarizing functions");

    let mut pending_symbols = Vec::new();
    let mut skipped = 0;
    for sym in &functions {
        // Skip if summary already exists
        if store.get_summary_for_symbol(sym.id)?.is_some() {
            skipped += 1;
            continue;
        }

        let body = match read_symbol_body(repo_root, &sym.file_path, sym.start_line, sym.end_line) {
            Ok(b) => b,
            Err(_) => continue,
        };

        pending_symbols.push(PendingSymbolSummary {
            symbol_id: sym.id,
            symbol_name: sym.name.clone(),
            user_prompt: prompt::function_prompt(&sym.file_path, &sym.name, &sym.kind, &body),
        });
    }

    // Collect function results and write to store
    for batch in pending_symbols.chunks(concurrency) {
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
                (pending.symbol_id, pending.symbol_name, result)
            }));
        }

        let mut batch_rate_limit_errors = 0;
        let mut batch_provider_pressure_errors = 0;
        for task in tasks {
            match task.await {
                Ok((sym_id, name, Ok(summary))) => {
                    let token_count = (summary.len() / summarizer::TOKEN_ESTIMATION_DIVISOR) as i32;
                    store.insert_summary(
                        Some(sym_id),
                        None,
                        summarizer::KIND_FUNCTION,
                        &summary,
                        Some(token_count),
                    )?;
                    symbols_summarized += 1;
                    tracing::debug!(symbol = %name, "Summarized");
                }
                Ok((_, name, Err(e))) => {
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
                "Stopping function summarization because provider pressure is sustained"
            );
            break;
        }
    }

    if skipped > 0 {
        tracing::info!(skipped, "Skipped already-summarized functions");
    }

    if provider_limited {
        tracing::warn!(
            "Skipping file summarization because function summarization hit provider pressure"
        );
        return Ok(SummarizeStats {
            symbols_summarized,
            files_summarized,
            errors,
            rate_limited,
            provider_limited,
            total_duration: start.elapsed(),
        });
    }

    // Batch 2: File-level summaries
    let file_ids: Vec<i64> = symbols
        .iter()
        .map(|s| s.file_id)
        .collect::<std::collections::HashSet<_>>()
        .into_iter()
        .collect();

    tracing::info!(count = file_ids.len(), "Summarizing files");

    // Build a map of file_id -> path
    let file_paths: HashMap<i64, String> = symbols
        .iter()
        .map(|s| (s.file_id, s.file_path.clone()))
        .collect();

    let mut pending_files = Vec::new();
    let mut files_skipped = 0;
    for file_id in &file_ids {
        // Skip if file-level summary already exists
        if store.get_summary_for_file(*file_id)?.is_some() {
            files_skipped += 1;
            continue;
        }

        let child_summaries = store.get_summaries_for_file_symbols(*file_id)?;
        if child_summaries.is_empty() {
            continue;
        }

        let file_path = match file_paths.get(file_id) {
            Some(p) => p.clone(),
            None => continue,
        };

        let children: Vec<(String, String)> = child_summaries
            .iter()
            .map(|(_, name, _, summary)| (name.clone(), summary.clone()))
            .collect();

        pending_files.push(PendingFileSummary {
            file_id: *file_id,
            user_prompt: prompt::file_prompt(&file_path, &children),
        });
    }

    if files_skipped > 0 {
        tracing::info!(skipped = files_skipped, "Skipped already-summarized files");
    }

    for batch in pending_files.chunks(concurrency) {
        let mut tasks = Vec::new();
        for pending in batch {
            let client = Arc::clone(&client);
            let sem = Arc::clone(&semaphore);
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
                (pending.file_id, result)
            }));
        }

        let mut batch_rate_limit_errors = 0;
        let mut batch_provider_pressure_errors = 0;
        for task in tasks {
            match task.await {
                Ok((file_id, Ok(summary))) => {
                    let token_count = (summary.len() / summarizer::TOKEN_ESTIMATION_DIVISOR) as i32;
                    store.insert_summary(
                        None,
                        Some(file_id),
                        summarizer::LEVEL_FILE,
                        &summary,
                        Some(token_count),
                    )?;
                    files_summarized += 1;
                }
                Ok((file_id, Err(e))) => {
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
            start.elapsed(),
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

    Ok(SummarizeStats {
        symbols_summarized,
        files_summarized,
        errors,
        rate_limited,
        provider_limited,
        total_duration: start.elapsed(),
    })
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
