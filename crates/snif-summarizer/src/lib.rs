mod prompt;

use anyhow::{bail, Result};
use snif_config::ModelConfig;
use snif_execution::LlmClient;
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
    pub total_duration: Duration,
}

pub fn summarize_all(
    store: &Store,
    repo_root: &Path,
    config: &ModelConfig,
) -> Result<SummarizeStats> {
    let api_key = std::env::var("SNIF_API_KEY")
        .or_else(|_| std::env::var("OPENAI_API_KEY"))
        .unwrap_or_default();

    if api_key.is_empty() {
        tracing::warn!(
            "No API key found (SNIF_API_KEY or OPENAI_API_KEY). Skipping summarization."
        );
        return Ok(SummarizeStats {
            symbols_summarized: 0,
            files_summarized: 0,
            errors: 0,
            total_duration: Duration::ZERO,
        });
    }

    if config.endpoint.is_empty() || config.summary_model.is_empty() {
        tracing::warn!("No endpoint or summary_model configured. Skipping summarization.");
        return Ok(SummarizeStats {
            symbols_summarized: 0,
            files_summarized: 0,
            errors: 0,
            total_duration: Duration::ZERO,
        });
    }

    let rt = tokio::runtime::Runtime::new()?;
    rt.block_on(summarize_all_async(store, repo_root, config, &api_key))
}

async fn summarize_all_async(
    store: &Store,
    repo_root: &Path,
    config: &ModelConfig,
    api_key: &str,
) -> Result<SummarizeStats> {
    let start = Instant::now();
    let client = Arc::new(LlmClient::from_config(config, api_key, false));
    let semaphore = Arc::new(Semaphore::new(5));

    let symbols = store.get_symbols_for_summarization()?;
    tracing::info!(symbols = symbols.len(), "Starting summarization");

    let mut symbols_summarized = 0;
    let mut files_summarized = 0;
    let mut errors = 0;

    // Batch 1: Functions and methods
    let functions: Vec<_> = symbols
        .iter()
        .filter(|s| s.kind == "function" || s.kind == "method")
        .collect();

    tracing::info!(count = functions.len(), "Summarizing functions");

    let mut tasks = Vec::new();
    for sym in &functions {
        let body = match read_symbol_body(repo_root, &sym.file_path, sym.start_line, sym.end_line) {
            Ok(b) => b,
            Err(_) => continue,
        };

        let client = Arc::clone(&client);
        let sem = Arc::clone(&semaphore);
        let user_prompt = prompt::function_prompt(&sym.file_path, &sym.name, &sym.kind, &body);
        let sym_id = sym.id;
        let sym_name = sym.name.clone();

        tasks.push(tokio::spawn(async move {
            let _permit = sem.acquire().await.unwrap();
            let result = client
                .chat_completion(prompt::SYSTEM_PROMPT, &user_prompt)
                .await;
            (sym_id, sym_name, result)
        }));
    }

    // Collect function results and write to store
    for task in tasks {
        match task.await {
            Ok((sym_id, name, Ok(summary))) => {
                let token_count = (summary.len() / 4) as i32;
                store.insert_summary(
                    Some(sym_id),
                    None,
                    "function",
                    &summary,
                    Some(token_count),
                )?;
                symbols_summarized += 1;
                tracing::debug!(symbol = %name, "Summarized");
            }
            Ok((_, name, Err(e))) => {
                tracing::warn!(symbol = %name, error = %e, "Failed to summarize");
                errors += 1;
            }
            Err(e) => {
                tracing::warn!(error = %e, "Task panicked");
                errors += 1;
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

    tracing::info!(count = file_ids.len(), "Summarizing files");

    // Build a map of file_id -> path
    let file_paths: HashMap<i64, String> = symbols
        .iter()
        .map(|s| (s.file_id, s.file_path.clone()))
        .collect();

    let mut file_tasks = Vec::new();
    for file_id in &file_ids {
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

        let client = Arc::clone(&client);
        let sem = Arc::clone(&semaphore);
        let user_prompt = prompt::file_prompt(&file_path, &children);
        let fid = *file_id;

        file_tasks.push(tokio::spawn(async move {
            let _permit = sem.acquire().await.unwrap();
            let result = client
                .chat_completion(prompt::SYSTEM_PROMPT, &user_prompt)
                .await;
            (fid, result)
        }));
    }

    for task in file_tasks {
        match task.await {
            Ok((file_id, Ok(summary))) => {
                let token_count = (summary.len() / 4) as i32;
                store.insert_summary(None, Some(file_id), "file", &summary, Some(token_count))?;
                files_summarized += 1;
            }
            Ok((file_id, Err(e))) => {
                tracing::warn!(file_id, error = %e, "Failed to summarize file");
                errors += 1;
            }
            Err(e) => {
                tracing::warn!(error = %e, "File task panicked");
                errors += 1;
            }
        }
    }

    Ok(SummarizeStats {
        symbols_summarized,
        files_summarized,
        errors,
        total_duration: start.elapsed(),
    })
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
