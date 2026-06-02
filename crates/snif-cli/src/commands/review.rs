use anyhow::{bail, Context, Result};
use snif_config::constants::cli;
use snif_config::env::{app, ci};
use snif_platform::PlatformAdapter;
use snif_types::{ContentTier, ContextPackage};
use std::path::Path;

const REVIEW_RATE_LIMIT_PROMPT_TARGETS: [usize; 3] = [64_000, 48_000, 32_000];

fn print_sarif_output(json: &str) {
    println!("{}", json);
}

fn print_findings_output(json: &str) {
    if !json.is_empty() {
        println!("{}", json);
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct PromptTrimStats {
    related_files_removed: usize,
    changed_files_degraded: usize,
}

struct RenderedReviewPrompts {
    system_prompt: String,
    user_prompt: String,
    prompt_tokens: usize,
    related_files: usize,
    trim_stats: PromptTrimStats,
}

pub fn run(
    path: &str,
    platform: Option<&str>,
    repo: Option<&str>,
    pr: Option<u64>,
    project: Option<&str>,
    diff_file: Option<&str>,
    format: &str,
) -> Result<()> {
    let repo_path = Path::new(path);
    tracing::info!(path = %repo_path.display(), "Starting review");

    let config = snif_config::SnifConfig::load(repo_path)?;
    let store = snif_store::Store::open(Path::new(&config.index.db_path))?;

    // Detect platform
    let detected_platform = detect_platform(platform, &config.platform.provider);
    tracing::info!(platform = %detected_platform, "Platform detected");

    // Get diff and metadata — either from a file or from the platform adapter
    let (diff, metadata, changed_paths, adapter): (
        String,
        snif_types::ChangeMetadata,
        Vec<String>,
        Option<Box<dyn PlatformAdapter>>,
    ) = if let Some(diff_path) = diff_file {
        let diff = std::fs::read_to_string(diff_path).context("Failed to read diff file")?;
        let paths = snif_platform::parse_changed_paths_from_diff(&diff);
        let metadata = snif_types::ChangeMetadata::default();
        (diff, metadata, paths, None)
    } else {
        let adapter = create_adapter(
            &detected_platform,
            repo,
            pr,
            project,
            config.platform.api_base.as_deref(),
        )?;
        let diff = adapter.fetch_diff()?;
        let metadata = adapter.fetch_metadata()?;
        let paths = adapter.fetch_changed_paths()?;
        (diff, metadata, paths, Some(adapter))
    };

    tracing::info!(
        changed_files = changed_paths.len(),
        diff_lines = diff.lines().count(),
        "Change loaded"
    );

    // Extract identifiers from the diff for keyword retrieval
    let diff_identifiers = snif_platform::extract_identifiers_from_diff(&diff);

    // Retrieve related files from the index
    let embedder = snif_embeddings::Embedder::new()?;
    let retrieval_results = snif_retrieval::retrieve(
        &store,
        &changed_paths,
        &diff_identifiers,
        &embedder,
        &config.context.retrieval_weights,
    )?;

    tracing::info!(
        structural = retrieval_results.structural_count,
        semantic = retrieval_results.semantic_count,
        keyword = retrieval_results.keyword_count,
        total = retrieval_results.results.len(),
        "Retrieval complete"
    );

    // Build context package
    let mut context = snif_context::build_context(
        &diff,
        &changed_paths,
        &retrieval_results,
        repo_path,
        &store,
        &config.context,
        metadata,
    )?;

    let initial_prompt_budget = config
        .context
        .max_tokens
        .saturating_sub(config.context.output_reserve_tokens);
    let mut rendered = render_prompts_for_prompt_budget(
        &config,
        &mut context,
        initial_prompt_budget,
        "Prompt exceeds budget",
    );

    tracing::info!(
        system_tokens = snif_context::budget::estimate_tokens(&rendered.system_prompt),
        user_tokens = snif_context::budget::estimate_tokens(&rendered.user_prompt),
        related_files = rendered.related_files,
        "Prompts rendered"
    );

    // Execute review via LLM
    let original_prompt_tokens = rendered.prompt_tokens;
    let mut review_context_note = None;
    let result = match snif_execution::execute_review(
        &rendered.system_prompt,
        &rendered.user_prompt,
        &config.model,
    ) {
        Ok(result) => result,
        Err(error) if snif_execution::is_rate_limit_error(&error) => {
            let mut last_error = error;
            let mut result = None;

            for target in fallback_prompt_targets(original_prompt_tokens) {
                let previous_tokens = rendered.prompt_tokens;
                let fallback = render_prompts_for_prompt_budget(
                    &config,
                    &mut context,
                    target,
                    "Provider rate-limited review request, trimming context for retry",
                );

                if fallback.prompt_tokens >= previous_tokens {
                    rendered = fallback;
                    continue;
                }

                tracing::warn!(
                    original_prompt_tokens,
                    target_prompt_tokens = target,
                    prompt_tokens = fallback.prompt_tokens,
                    related_files = fallback.related_files,
                    related_files_removed = fallback.trim_stats.related_files_removed,
                    changed_files_degraded = fallback.trim_stats.changed_files_degraded,
                    "Retrying review with reduced context after provider rate limit"
                );

                match snif_execution::execute_review(
                    &fallback.system_prompt,
                    &fallback.user_prompt,
                    &config.model,
                ) {
                    Ok(ok) => {
                        review_context_note = Some(format!(
                            "Context was reduced after provider rate limiting (prompt tokens {} -> {}).",
                            original_prompt_tokens, fallback.prompt_tokens
                        ));
                        result = Some(ok);
                        break;
                    }
                    Err(error) if snif_execution::is_rate_limit_error(&error) => {
                        last_error = error;
                        rendered = fallback;
                    }
                    Err(error) => return Err(error),
                }
            }

            match result {
                Some(result) => result,
                None => bail!(
                    "Review request was rate-limited even after reduced-context retries. Last error: {}. Try lowering context.max_tokens and/or context.summarizer_concurrency.",
                    last_error
                ),
            }
        }
        Err(error) => return Err(error),
    };

    tracing::info!(
        duration = ?result.duration,
        response_len = result.response.len(),
        "LLM execution complete"
    );

    // Parse findings from LLM response
    let mut parsed = snif_output::parser::parse_response(&result.response)?;

    // Run repair if findings are empty OR if chain-of-thought leakage is detected
    let needs_repair =
        parsed.findings.is_empty() || snif_output::parser::has_chain_of_thought(&result.response);

    if needs_repair {
        tracing::warn!("Repairing review response");
        let repaired = snif_execution::repair_review_response(&result.response, &config.model)?;
        parsed = snif_output::parser::parse_response(&repaired.response)?;
    }

    let change_summary = parsed.summary;
    let mut findings = parsed.findings;

    // Apply static filters
    findings = snif_output::filter::apply_filters(findings, &config.filter);

    // Compute fingerprints
    snif_output::fingerprint::compute_fingerprints(&mut findings);

    // Output findings
    if findings.is_empty() {
        tracing::info!("No findings — change looks clean");
    } else {
        tracing::info!(count = findings.len(), "Findings after filtering");
    }

    match format {
        cli::OUTPUT_FORMAT_SARIF => {
            let sarif = snif_output::sarif::to_sarif(&findings);
            let sarif_json = serde_json::to_string_pretty(&sarif)?;
            std::fs::write("findings.sarif", &sarif_json)?;
            print_sarif_output(&sarif_json);
        }
        _ => {
            if !findings.is_empty() {
                let json = serde_json::to_string_pretty(&findings)?;
                print_findings_output(&json);
            }
        }
    }

    // Post to platform if adapter is available
    if let Some(adapter) = &adapter {
        let prior = adapter.get_prior_fingerprints()?;

        adapter.post_findings(&findings)?;

        let summary =
            snif_output::summary::format_pr_summary(&snif_output::summary::ReviewSummaryInput {
                change_summary: &change_summary,
                findings: &findings,
                changed_paths: &changed_paths,
                retrieval_results: &retrieval_results,
                diff_lines: diff.lines().count(),
                model_name: &config.model.review_model,
                duration_secs: result.duration.as_secs(),
                context_note: review_context_note.as_deref(),
            });
        adapter.post_summary(&summary)?;

        tracing::info!(posted = findings.len(), "Findings posted");

        // Only resolve stale findings when the current review produced findings.
        // If current review is completely clean (zero findings), don't auto-resolve
        // prior ones — the LLM likely just missed them this time. Conservative —
        // biases toward keeping findings visible rather than incorrectly clearing them.
        if findings.is_empty() {
            tracing::info!(
                "Current review is clean — skipping stale resolution to avoid clearing prior findings"
            );
        } else {
            // Collect both content-based and line-based IDs from current findings
            let current_content_ids: std::collections::HashSet<&str> = findings
                .iter()
                .filter_map(|f| f.fingerprint.as_ref().map(|fp| fp.id.as_str()))
                .collect();
            let current_line_ids: std::collections::HashSet<&str> = findings
                .iter()
                .filter_map(|f| f.fingerprint.as_ref().map(|fp| fp.line_id.as_str()))
                .collect();

            // A prior finding is stale only if NEITHER its content hash NOR its
            // line hash matches any current finding. Conservative — biases toward
            // keeping findings rather than incorrectly resolving them.
            let stale: Vec<_> = prior
                .into_iter()
                .filter(|fp| {
                    !current_content_ids.contains(fp.id.as_str())
                        && !current_line_ids.contains(fp.id.as_str())
                        && !current_content_ids.contains(fp.line_id.as_str())
                        && !current_line_ids.contains(fp.line_id.as_str())
                })
                .collect();

            if !stale.is_empty() {
                tracing::info!(count = stale.len(), "Resolving stale findings");
                adapter.resolve_stale(&stale)?;
            }
        }
    }

    Ok(())
}

fn render_prompts_for_prompt_budget(
    config: &snif_config::SnifConfig,
    context: &mut ContextPackage,
    prompt_budget: usize,
    reason: &str,
) -> RenderedReviewPrompts {
    let mut trim_stats = PromptTrimStats::default();

    loop {
        let system_prompt = snif_prompts::render_system_prompt(config);
        let user_prompt = snif_prompts::render_user_prompt(context);
        let prompt_tokens = snif_context::budget::estimate_tokens(&system_prompt)
            + snif_context::budget::estimate_tokens(&user_prompt);

        if prompt_tokens <= prompt_budget {
            return RenderedReviewPrompts {
                system_prompt,
                user_prompt,
                prompt_tokens,
                related_files: context.related_files.len(),
                trim_stats,
            };
        }

        if context.related_files.pop().is_some() {
            trim_stats.related_files_removed += 1;
            tracing::warn!(
                tokens = prompt_tokens,
                prompt_budget,
                remaining_files = context.related_files.len(),
                reason,
                "Trimming related file from review prompt"
            );
            continue;
        }

        let largest_full = context
            .changed_files
            .iter()
            .enumerate()
            .filter(|(_, f)| f.content_tier == ContentTier::Full)
            .max_by_key(|(_, f)| f.content.len());

        if let Some((idx, _)) = largest_full {
            let file = &mut context.changed_files[idx];
            trim_stats.changed_files_degraded += 1;
            tracing::warn!(
                path = %file.path,
                content_bytes = file.content.len(),
                reason,
                "Degrading changed file to diff-only to fit review prompt budget"
            );
            file.content = cli::CONTENT_DIFF_ONLY_PLACEHOLDER.to_string();
            file.content_tier = ContentTier::DiffOnly;
            file.summary = None;
            continue;
        }

        tracing::warn!(
            tokens = prompt_tokens,
            prompt_budget,
            reason,
            "Prompt still exceeds budget after all degradation"
        );
        return RenderedReviewPrompts {
            system_prompt,
            user_prompt,
            prompt_tokens,
            related_files: context.related_files.len(),
            trim_stats,
        };
    }
}

fn fallback_prompt_targets(current_prompt_tokens: usize) -> impl Iterator<Item = usize> {
    REVIEW_RATE_LIMIT_PROMPT_TARGETS
        .into_iter()
        .filter(move |target| *target < current_prompt_tokens)
}

#[cfg(test)]
mod tests {
    use super::*;
    use snif_types::{BudgetReport, ChangeMetadata, ContextFile};

    #[test]
    fn fallback_targets_only_include_smaller_prompt_budgets() {
        assert_eq!(
            fallback_prompt_targets(91_184).collect::<Vec<_>>(),
            vec![64_000, 48_000, 32_000]
        );
        assert_eq!(
            fallback_prompt_targets(50_000).collect::<Vec<_>>(),
            vec![48_000, 32_000]
        );
        assert_eq!(
            fallback_prompt_targets(32_000).collect::<Vec<_>>(),
            Vec::<usize>::new()
        );
    }

    #[test]
    fn prompt_trimming_removes_related_files_before_degrading_changed_files() {
        let config = snif_config::SnifConfig::default();
        let mut context = ContextPackage {
            metadata: ChangeMetadata::default(),
            diff: "diff --git a/src/main.rs b/src/main.rs\n".to_string(),
            changed_files: vec![ContextFile {
                path: "src/main.rs".to_string(),
                content: "fn main() {}".to_string(),
                summary: None,
                retrieval_score: None,
                content_tier: ContentTier::Full,
            }],
            related_files: vec![
                ContextFile {
                    path: "src/a.rs".to_string(),
                    content: "a".repeat(12_000),
                    summary: None,
                    retrieval_score: Some(1.0),
                    content_tier: ContentTier::Full,
                },
                ContextFile {
                    path: "src/b.rs".to_string(),
                    content: "b".repeat(12_000),
                    summary: None,
                    retrieval_score: Some(0.9),
                    content_tier: ContentTier::Full,
                },
            ],
            omissions: Vec::new(),
            budget: BudgetReport {
                total_budget: 0,
                diff_tokens: 0,
                changed_files_tokens: 0,
                related_files_tokens: 0,
                remaining_tokens: 0,
                files_included: 0,
                files_omitted: 0,
                files_full: 0,
                files_summary_only: 0,
                files_diff_only: 0,
            },
        };

        let rendered =
            render_prompts_for_prompt_budget(&config, &mut context, 8_000, "test trimming");

        assert!(rendered.trim_stats.related_files_removed > 0);
        assert_eq!(rendered.trim_stats.changed_files_degraded, 0);
        assert_eq!(context.changed_files[0].content_tier, ContentTier::Full);
    }
}

fn detect_platform(explicit: Option<&str>, config_default: &str) -> String {
    if let Some(p) = explicit {
        return p.to_string();
    }
    if let Ok(p) = std::env::var(app::SNIF_PLATFORM) {
        return p;
    }
    if std::env::var(ci::CI_PROJECT_PATH).is_ok() {
        return cli::PLATFORM_GITLAB.to_string();
    }
    if std::env::var(ci::GITHUB_REPOSITORY).is_ok() {
        return cli::PLATFORM_GITHUB.to_string();
    }
    config_default.to_string()
}

fn create_adapter(
    platform: &str,
    repo: Option<&str>,
    pr: Option<u64>,
    project: Option<&str>,
    config_api_base: Option<&str>,
) -> Result<Box<dyn PlatformAdapter>> {
    match platform {
        cli::PLATFORM_GITLAB => {
            let project_path = project
                .or(repo)
                .map(String::from)
                .or_else(|| std::env::var(ci::CI_PROJECT_PATH).ok())
                .context(cli::GITLAB_PROJECT_PATH_REQUIRED)?;
            let mr_iid = pr
                .or_else(|| {
                    std::env::var(ci::CI_MERGE_REQUEST_IID)
                        .ok()
                        .and_then(|s| s.parse().ok())
                })
                .context(cli::GITLAB_MR_IID_REQUIRED)?;
            let api_base = config_api_base
                .map(String::from)
                .or_else(|| std::env::var(ci::CI_API_V4_URL).ok());
            Ok(Box::new(snif_platform::gitlab::GitLabAdapter::new(
                &project_path,
                mr_iid,
                api_base.as_deref(),
            )?))
        }
        _ => {
            let repo_str = repo
                .map(String::from)
                .or_else(|| std::env::var(ci::GITHUB_REPOSITORY).ok())
                .context(cli::GITHUB_REPOSITORY_REQUIRED)?;
            let pr_num = pr
                .or_else(|| {
                    std::env::var(app::SNIF_PR_NUMBER)
                        .ok()
                        .and_then(|s| s.parse().ok())
                })
                .context(cli::SNIF_PR_NUMBER_REQUIRED)?;
            let parts: Vec<&str> = repo_str.splitn(2, '/').collect();
            if parts.len() != 2 {
                anyhow::bail!(cli::REPO_FORMAT_ERROR);
            }
            Ok(Box::new(snif_platform::github::GitHubAdapter::new(
                parts[0], parts[1], pr_num,
            )?))
        }
    }
}
