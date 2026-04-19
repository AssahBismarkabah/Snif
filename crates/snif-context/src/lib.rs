pub mod budget;

use anyhow::Result;
use snif_config::constants::{context, limits};
use snif_config::ContextConfig;
use snif_store::Store;
use snif_types::{
    BudgetReport, ChangeMetadata, ContentTier, ContextFile, ContextPackage, Omission,
    RetrievalResults,
};
use std::collections::HashMap;
use std::path::Path;

fn is_non_reviewable(path: &str) -> bool {
    let filename = path.rsplit('/').next().unwrap_or(path);
    context::NON_REVIEWABLE_FILES.contains(&filename)
        || context::NON_REVIEWABLE_EXTENSIONS
            .iter()
            .any(|ext| filename.ends_with(ext))
}

/// Count diff hunks per file path from a unified diff.
/// Handles multiple diff formats: +++ b/, +++ a/, +++ , and no-prefix.
fn count_hunks_per_file(diff: &str) -> HashMap<String, usize> {
    let mut counts: HashMap<String, usize> = HashMap::new();
    let mut current_file: Option<String> = None;

    for line in diff.lines() {
        // Handle various diff prefix formats: +++ b/path, +++ a/path, +++ path
        let path = line
            .strip_prefix("+++ b/")
            .or_else(|| line.strip_prefix("+++ a/"))
            .or_else(|| line.strip_prefix("+++ "));

        if let Some(p) = path {
            if p != "/dev/null" {
                current_file = Some(p.to_string());
            }
        } else if line.starts_with("+++ /dev/null") {
            current_file = None;
        } else if line.starts_with("@@") {
            if let Some(ref file) = current_file {
                *counts.entry(file.clone()).or_insert(0) += 1;
            }
        }
    }
    counts
}

struct FileCandidate {
    path: String,
    full_content: String,
    summary: Option<String>,
    full_tokens: usize,
    hunk_count: usize,
    forced_exclude: bool,
}

fn get_file_summary(store: &Store, path: &str) -> Option<String> {
    store
        .get_file_id(path)
        .ok()
        .flatten()
        .and_then(|fid| store.get_summary_for_file(fid).ok().flatten())
        .map(|(_, text)| text)
}

pub fn build_context(
    diff: &str,
    changed_paths: &[String],
    retrieval_results: &RetrievalResults,
    repo_root: &Path,
    store: &Store,
    config: &ContextConfig,
    metadata: ChangeMetadata,
) -> Result<ContextPackage> {
    let total_budget = config
        .max_tokens
        .saturating_sub(config.output_reserve_tokens);
    let mut remaining = total_budget;

    // Always include the diff
    let diff_tokens = budget::estimate_tokens(diff);
    remaining = remaining.saturating_sub(diff_tokens);

    if diff_tokens >= total_budget {
        tracing::warn!(
            diff_tokens,
            budget = total_budget,
            "Diff alone exceeds token budget — file content will be excluded"
        );
    }

    // Count hunks per file for prioritization
    let hunk_counts = count_hunks_per_file(diff);

    // Pass 1: Read all changed files and compute costs
    let mut candidates: Vec<FileCandidate> = Vec::new();

    for path in changed_paths {
        let full_path = repo_root.join(path);

        let file_size = std::fs::metadata(&full_path)
            .map(|m| m.len() as usize)
            .unwrap_or(0);

        let (full_content, forced_exclude) =
            if is_non_reviewable(path) || file_size > limits::MAX_CHANGED_FILE_BYTES {
                tracing::info!(
                    path = %path,
                    size = file_size,
                    "Skipping full content — large or non-reviewable file"
                );
                (String::new(), true)
            } else {
                let content = std::fs::read_to_string(&full_path).unwrap_or_default();
                (content, false)
            };

        let summary = get_file_summary(store, path);

        let full_tokens = budget::estimate_tokens(&full_content)
            + summary
                .as_ref()
                .map(|s| budget::estimate_tokens(s))
                .unwrap_or(0);
        let hunk_count = hunk_counts.get(path.as_str()).copied().unwrap_or(0);
        candidates.push(FileCandidate {
            path: path.to_string(),
            full_content,
            summary,
            full_tokens,
            hunk_count,
            forced_exclude,
        });
    }

    // Pass 2: Sort by hunk count descending (most changed files get priority)
    // Stable sort preserves original order for equal hunk counts
    candidates.sort_by_key(|c| std::cmp::Reverse(c.hunk_count));

    let mut changed_files = Vec::new();
    let mut changed_files_tokens = 0;
    let mut omissions = Vec::new();
    let mut files_full = 0_usize;
    let mut files_summary_only = 0_usize;
    let mut files_diff_only = 0_usize;

    let diff_only_placeholder = context::CONTENT_DIFF_ONLY_PLACEHOLDER;
    let diff_only_tokens = budget::estimate_tokens(diff_only_placeholder);

    for candidate in candidates {
        // Non-reviewable/large files always go to DiffOnly
        if candidate.forced_exclude {
            let content = context::CONTENT_EXCLUDED_PLACEHOLDER;
            let tokens = budget::estimate_tokens(content);
            changed_files_tokens += tokens;
            remaining = remaining.saturating_sub(tokens);
            files_diff_only += 1;
            changed_files.push(ContextFile {
                path: candidate.path,
                content: content.to_string(),
                summary: None,
                retrieval_score: None,
                content_tier: ContentTier::DiffOnly,
            });
            continue;
        }

        // Try full content first
        if remaining >= candidate.full_tokens {
            remaining -= candidate.full_tokens;
            changed_files_tokens += candidate.full_tokens;
            files_full += 1;
            changed_files.push(ContextFile {
                path: candidate.path,
                content: candidate.full_content,
                summary: candidate.summary,
                retrieval_score: None,
                content_tier: ContentTier::Full,
            });
            continue;
        }

        // Try summary-only
        if let Some(ref summary) = candidate.summary {
            let summary_tokens = budget::estimate_tokens(summary);
            if remaining >= summary_tokens {
                remaining -= summary_tokens;
                changed_files_tokens += summary_tokens;
                files_summary_only += 1;
                omissions.push(Omission {
                    path: candidate.path.clone(),
                    score: 0.0,
                    reason: context::REASON_CONTENT_DEGRADED_TO_SUMMARY.to_string(),
                });
                changed_files.push(ContextFile {
                    path: candidate.path,
                    content: format!("{}{}", context::SUMMARY_ONLY_CONTENT_PREFIX, summary),
                    summary: None,
                    retrieval_score: None,
                    content_tier: ContentTier::SummaryOnly,
                });
                continue;
            }
        }

        // Fall back to diff-only placeholder
        remaining = remaining.saturating_sub(diff_only_tokens);
        changed_files_tokens += diff_only_tokens;
        files_diff_only += 1;
        omissions.push(Omission {
            path: candidate.path.clone(),
            score: 0.0,
            reason: context::REASON_CONTENT_DEGRADED_TO_DIFF_ONLY.to_string(),
        });
        changed_files.push(ContextFile {
            path: candidate.path,
            content: diff_only_placeholder.to_string(),
            summary: None,
            retrieval_score: None,
            content_tier: ContentTier::DiffOnly,
        });
    }

    tracing::info!(
        full = files_full,
        summary_only = files_summary_only,
        diff_only = files_diff_only,
        "Changed files content tiers"
    );

    // Fill with related files from retrieval results
    let mut related_files = Vec::new();
    let mut related_files_tokens = 0;
    let mut files_included = 0;

    for result in &retrieval_results.results {
        if files_included >= config.max_files {
            omissions.push(Omission {
                path: result.path.clone(),
                score: result.score,
                reason: context::REASON_MAX_FILES_EXCEEDED.to_string(),
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
                reason: context::REASON_TOKEN_BUDGET_EXCEEDED.to_string(),
            });
            continue;
        }

        remaining -= tokens;
        related_files_tokens += tokens;
        files_included += 1;

        let summary = get_file_summary(store, &result.path);

        related_files.push(ContextFile {
            path: result.path.clone(),
            content,
            summary,
            retrieval_score: Some(result.score),
            content_tier: ContentTier::Full,
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
        files_full,
        files_summary_only,
        files_diff_only,
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
