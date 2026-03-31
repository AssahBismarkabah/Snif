pub mod budget;

use anyhow::Result;
use snif_config::ContextConfig;
use snif_store::Store;
use snif_types::{
    BudgetReport, ChangeMetadata, ContextFile, ContextPackage, Omission, RetrievalResults,
};
use std::path::Path;

pub fn build_context(
    diff: &str,
    changed_paths: &[String],
    retrieval_results: &RetrievalResults,
    repo_root: &Path,
    store: &Store,
    config: &ContextConfig,
    metadata: ChangeMetadata,
) -> Result<ContextPackage> {
    let total_budget = config.max_tokens;
    let mut remaining = total_budget;

    // Always include the diff
    let diff_tokens = budget::estimate_tokens(diff);
    remaining = remaining.saturating_sub(diff_tokens);

    // Include changed files
    let mut changed_files = Vec::new();
    let mut changed_files_tokens = 0;

    for path in changed_paths {
        let full_path = repo_root.join(path);
        let content = std::fs::read_to_string(&full_path).unwrap_or_default();
        let tokens = budget::estimate_tokens(&content);
        changed_files_tokens += tokens;
        remaining = remaining.saturating_sub(tokens);

        let summary = store
            .get_file_id(path)
            .ok()
            .flatten()
            .and_then(|fid| store.get_summary_for_file(fid).ok().flatten())
            .map(|(_, text)| text);

        changed_files.push(ContextFile {
            path: path.clone(),
            content,
            summary,
            retrieval_score: None,
        });
    }

    // Fill with related files from retrieval results
    let mut related_files = Vec::new();
    let mut omissions = Vec::new();
    let mut related_files_tokens = 0;
    let mut files_included = 0;

    for result in &retrieval_results.results {
        if files_included >= config.max_files {
            omissions.push(Omission {
                path: result.path.clone(),
                score: result.score,
                reason: "max_files_exceeded".to_string(),
            });
            continue;
        }

        let full_path = repo_root.join(&result.path);
        let content = match std::fs::read_to_string(&full_path) {
            Ok(c) => c,
            Err(_) => continue,
        };

        let tokens = budget::estimate_tokens(&content);

        if tokens > remaining {
            omissions.push(Omission {
                path: result.path.clone(),
                score: result.score,
                reason: "token_budget_exceeded".to_string(),
            });
            continue;
        }

        remaining -= tokens;
        related_files_tokens += tokens;
        files_included += 1;

        let summary = store
            .get_file_id(&result.path)
            .ok()
            .flatten()
            .and_then(|fid| store.get_summary_for_file(fid).ok().flatten())
            .map(|(_, text)| text);

        related_files.push(ContextFile {
            path: result.path.clone(),
            content,
            summary,
            retrieval_score: Some(result.score),
        });
    }

    let budget_report = BudgetReport {
        total_budget,
        diff_tokens,
        changed_files_tokens,
        related_files_tokens,
        remaining_tokens: remaining,
        files_included: changed_files.len() + files_included,
        files_omitted: omissions.len(),
    };

    tracing::info!(
        changed = changed_files.len(),
        related = related_files.len(),
        omitted = omissions.len(),
        remaining_tokens = remaining,
        "Context assembled"
    );

    Ok(ContextPackage {
        metadata,
        diff: diff.to_string(),
        changed_files,
        related_files,
        omissions,
        budget: budget_report,
    })
}
