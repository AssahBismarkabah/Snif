pub mod budget;

use anyhow::Result;
use snif_config::ContextConfig;
use snif_store::Store;
use snif_types::{
    BudgetReport, ChangeMetadata, ContentTier, ContextFile, ContextPackage, Omission,
    RetrievalResults,
};
use std::collections::HashMap;
use std::path::Path;

const MAX_CHANGED_FILE_BYTES: usize = 50_000;

const NON_REVIEWABLE_FILES: &[&str] = &[
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

fn is_non_reviewable(path: &str) -> bool {
    let filename = path.rsplit('/').next().unwrap_or(path);
    NON_REVIEWABLE_FILES.contains(&filename)
        || filename.ends_with(".lock")
        || filename.ends_with(".min.js")
        || filename.ends_with(".min.css")
        || filename.ends_with(".bundle.js")
}

/// Count diff hunks per file path from a unified diff.
fn count_hunks_per_file(diff: &str) -> HashMap<String, usize> {
    let mut counts: HashMap<String, usize> = HashMap::new();
    let mut current_file: Option<String> = None;

    for line in diff.lines() {
        if let Some(path) = line.strip_prefix("+++ b/") {
            if path != "/dev/null" {
                current_file = Some(path.to_string());
            }
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
    let total_budget = config.max_tokens;
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
            if is_non_reviewable(path) || file_size > MAX_CHANGED_FILE_BYTES {
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

        let summary = store
            .get_file_id(path)
            .ok()
            .flatten()
            .and_then(|fid| store.get_summary_for_file(fid).ok().flatten())
            .map(|(_, text)| text);

        if forced_exclude {
            // Non-reviewable files always go to DiffOnly — no budget competition
            candidates.push(FileCandidate {
                path: path.clone(),
                full_content: String::new(),
                summary,
                full_tokens: 0,
                hunk_count: 0,
            });
        } else {
            let full_tokens = budget::estimate_tokens(&full_content);
            let hunk_count = hunk_counts.get(path.as_str()).copied().unwrap_or(0);
            candidates.push(FileCandidate {
                path: path.clone(),
                full_content,
                summary,
                full_tokens,
                hunk_count,
            });
        }
    }

    // Pass 2: Sort by hunk count descending (most changed files get priority)
    // Stable sort preserves original order for equal hunk counts
    candidates.sort_by(|a, b| b.hunk_count.cmp(&a.hunk_count));

    let mut changed_files = Vec::new();
    let mut changed_files_tokens = 0;
    let mut omissions = Vec::new();
    let mut files_full = 0_usize;
    let mut files_summary_only = 0_usize;
    let mut files_diff_only = 0_usize;

    let diff_only_placeholder = "[See diff for changes to this file.]";
    let diff_only_tokens = budget::estimate_tokens(diff_only_placeholder);

    for candidate in candidates {
        // Non-reviewable files (full_tokens == 0 and empty content) always DiffOnly
        if candidate.full_tokens == 0 && candidate.full_content.is_empty() {
            let content =
                "[File content excluded — large or generated file. See diff for changes.]";
            let tokens = budget::estimate_tokens(content);
            changed_files_tokens += tokens;
            remaining = remaining.saturating_sub(tokens);
            files_diff_only += 1;
            changed_files.push(ContextFile {
                path: candidate.path,
                content: content.to_string(),
                summary: candidate.summary,
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
                    reason: "content_degraded_to_summary".to_string(),
                });
                changed_files.push(ContextFile {
                    path: candidate.path,
                    content: summary.clone(),
                    summary: candidate.summary,
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
            reason: "content_degraded_to_diff_only".to_string(),
        });
        changed_files.push(ContextFile {
            path: candidate.path,
            content: diff_only_placeholder.to_string(),
            summary: candidate.summary,
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
