mod prompt;

use anyhow::{bail, Result};
use snif_config::{constants::summarizer, env::keys, ModelConfig};
use snif_execution::{is_rate_limit_error, LlmClient};
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
                    .chat_completion(prompt::SYSTEM_PROMPT, &pending.user_prompt)
                    .await;
                (pending.symbol_id, pending.symbol_name, result)
            }));
        }

        let mut batch_rate_limit_errors = 0;
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

        if should_stop_after_rate_limit_batch(batch.len(), batch_rate_limit_errors) {
            rate_limited = true;
            tracing::warn!(
                concurrency,
                failed = batch_rate_limit_errors,
                "Stopping function summarization because provider rate-limited a full batch"
            );
            break;
        }
    }

    if skipped > 0 {
        tracing::info!(skipped, "Skipped already-summarized functions");
    }

    if rate_limited {
        tracing::warn!(
            "Skipping file summarization because function summarization was rate-limited"
        );
        return Ok(SummarizeStats {
            symbols_summarized,
            files_summarized,
            errors,
            rate_limited,
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
                    .chat_completion(prompt::SYSTEM_PROMPT, &pending.user_prompt)
                    .await;
                (pending.file_id, result)
            }));
        }

        let mut batch_rate_limit_errors = 0;
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

        if should_stop_after_rate_limit_batch(batch.len(), batch_rate_limit_errors) {
            rate_limited = true;
            tracing::warn!(
                concurrency,
                failed = batch_rate_limit_errors,
                "Stopping file summarization because provider rate-limited a full batch"
            );
            break;
        }
    }

    Ok(SummarizeStats {
        symbols_summarized,
        files_summarized,
        errors,
        rate_limited,
        total_duration: start.elapsed(),
    })
}

fn normalize_concurrency(concurrency: usize) -> usize {
    concurrency.max(1)
}

fn should_stop_after_rate_limit_batch(batch_len: usize, rate_limit_errors: usize) -> bool {
    batch_len > 0 && rate_limit_errors == batch_len
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
    fn full_rate_limited_batch_stops_summarization() {
        assert!(should_stop_after_rate_limit_batch(3, 3));
        assert!(should_stop_after_rate_limit_batch(1, 1));
    }

    #[test]
    fn partial_or_empty_rate_limited_batch_does_not_stop_summarization() {
        assert!(!should_stop_after_rate_limit_batch(3, 2));
        assert!(!should_stop_after_rate_limit_batch(3, 0));
        assert!(!should_stop_after_rate_limit_batch(0, 0));
    }
}
