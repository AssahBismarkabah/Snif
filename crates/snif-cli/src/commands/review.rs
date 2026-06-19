use anyhow::{bail, Context, Result};
use snif_config::constants::{cli, context as context_constants};
use snif_config::env::{app, ci};
use snif_config::ReviewInconclusiveMode;
use snif_output::integrity::InconclusiveReason;
use snif_output::summary::ReviewOutcome;
use snif_platform::PlatformAdapter;
use snif_types::{ContentTier, ContextPackage};
use std::path::Path;

const REVIEW_PROVIDER_PRESSURE_PROMPT_TARGETS: [usize; 6] =
    [64_000, 48_000, 32_000, 16_000, 8_000, 4_000];
const REVIEW_PROVIDER_PRESSURE_MIN_OUTPUT_TOKENS: usize = 1_024;
const DIFF_TRUNCATION_NOTICE: &str =
    "\n[Diff truncated after provider pressure to fit a reduced review budget.]\n";

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
    diff_truncated: bool,
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
    let mut review_context_note = None;
    let embedding_cache_dir = config.resolved_embedding_cache_dir(repo_path);
    let embedder = match snif_embeddings::Embedder::new_with_cache_dir(&embedding_cache_dir) {
        Ok(embedder) => Some(embedder),
        Err(error) if error.is_rate_limited() => {
            tracing::warn!(
                cache_dir = %embedding_cache_dir.display(),
                error = %error,
                "Continuing review without semantic retrieval because the embedding model download was rate-limited"
            );
            push_context_note(
                &mut review_context_note,
                "Semantic retrieval was skipped because Hugging Face rate-limited the embedding model download.",
            );
            None
        }
        Err(error) => return Err(error.into()),
    };
    let retrieval_results = snif_retrieval::retrieve(
        &store,
        &changed_paths,
        &diff_identifiers,
        embedder.as_ref(),
        &config.context.retrieval_weights,
    )?;

    tracing::info!(
        structural = retrieval_results.structural_count,
        semantic = retrieval_results.semantic_count,
        code_semantic = retrieval_results.code_semantic_count,
        keyword = retrieval_results.keyword_count,
        total = retrieval_results.results.len(),
        "Retrieval complete"
    );

    // On-demand summarization: check for files missing summaries and
    // generate them if a summarization API key is configured.
    {
        let api_key = std::env::var(snif_config::env::keys::SNIF_API_KEY)
            .or_else(|_| std::env::var(snif_config::env::keys::OPENAI_API_KEY))
            .unwrap_or_default();

        if !api_key.is_empty()
            && !config.model.endpoint.is_empty()
            && !config.model.summary_model.is_empty()
        {
            // Collect all file paths that might need summaries: changed files + retrieved related files
            let mut paths_needing_summaries: Vec<String> = changed_paths.clone();
            for result in &retrieval_results.results {
                if !paths_needing_summaries.contains(&result.path) {
                    paths_needing_summaries.push(result.path.clone());
                }
            }

            // Check which files lack summaries and generate them on demand
            let mut files_without_summaries = Vec::new();
            for path in &paths_needing_summaries {
                let has_summary = store
                    .get_file_id(path)
                    .ok()
                    .flatten()
                    .map(|fid| store.get_summary_for_file(fid).ok().flatten().is_some())
                    .unwrap_or(false);
                if !has_summary {
                    files_without_summaries.push(path.clone());
                }
            }

            if !files_without_summaries.is_empty() {
                tracing::info!(
                    count = files_without_summaries.len(),
                    "Generating on-demand summaries for files missing from index"
                );
                let summary_stats = snif_summarizer::summarize_files(
                    &store,
                    repo_path,
                    &config.model,
                    &files_without_summaries,
                    config.context.summarizer_concurrency,
                )?;

                tracing::info!(
                    symbols = summary_stats.symbols_summarized,
                    files = summary_stats.files_summarized,
                    symbols_skipped = summary_stats.symbols_skipped_unchanged,
                    files_skipped = summary_stats.files_skipped_unchanged,
                    errors = summary_stats.errors,
                    duration = ?summary_stats.total_duration,
                    "On-demand summarization complete"
                );

                // Re-embed any new summaries using the already-loaded embedder
                if snif_embeddings::has_summaries_missing_embeddings(&store)? {
                    if let Some(embedder) = embedder.as_ref() {
                        let embed_stats = snif_embeddings::embed_all_summaries(&store, embedder)?;
                        tracing::info!(
                            embedded = embed_stats.summaries_embedded,
                            dimension = embed_stats.dimension,
                            duration = ?embed_stats.duration,
                            "On-demand embedding complete"
                        );
                    } else {
                        tracing::warn!(
                            "Skipping on-demand embedding because the embedder is not available"
                        );
                    }
                }
            }
        } else {
            tracing::debug!("No API key configured, skipping on-demand summarization");
        }
    }

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
        false,
    );

    tracing::info!(
        system_tokens = snif_context::budget::estimate_tokens(&rendered.system_prompt),
        user_tokens = snif_context::budget::estimate_tokens(&rendered.user_prompt),
        related_files = rendered.related_files,
        "Prompts rendered"
    );

    // Execute review via LLM
    let original_prompt_tokens = rendered.prompt_tokens;
    let initial_output_max_tokens = review_output_max_tokens(&config, initial_prompt_budget);
    let mut successful_output_max_tokens = initial_output_max_tokens;
    let result = match snif_execution::execute_review_with_max_tokens_and_policy(
        &rendered.system_prompt,
        &rendered.user_prompt,
        &config.model,
        Some(initial_output_max_tokens),
        snif_execution::LlmRetryPolicy::SurfaceReducibleProviderErrors,
    ) {
        Ok(result) => result,
        Err(error) if snif_execution::is_reducible_provider_error(&error) => {
            let mut last_error = error;
            let mut result = None;
            let mut last_output_max_tokens = initial_output_max_tokens;

            for target in fallback_prompt_targets(original_prompt_tokens) {
                let previous_tokens = rendered.prompt_tokens;
                let output_max_tokens = fallback_output_max_tokens(&config, target);
                let fallback = render_prompts_for_prompt_budget(
                    &config,
                    &mut context,
                    target,
                    "Provider pressure on review request, trimming context for retry",
                    true,
                );

                if !fallback_reduces_request(
                    previous_tokens,
                    last_output_max_tokens,
                    fallback.prompt_tokens,
                    output_max_tokens,
                ) {
                    rendered = fallback;
                    continue;
                }

                tracing::warn!(
                    original_prompt_tokens,
                    target_prompt_tokens = target,
                    prompt_tokens = fallback.prompt_tokens,
                    output_max_tokens,
                    related_files = fallback.related_files,
                    related_files_removed = fallback.trim_stats.related_files_removed,
                    changed_files_degraded = fallback.trim_stats.changed_files_degraded,
                    diff_truncated = fallback.trim_stats.diff_truncated,
                    provider_failure = provider_failure_reason(&last_error),
                    "Retrying review with reduced context after provider pressure"
                );

                match snif_execution::execute_review_with_max_tokens_and_policy(
                    &fallback.system_prompt,
                    &fallback.user_prompt,
                    &config.model,
                    Some(output_max_tokens),
                    snif_execution::LlmRetryPolicy::SurfaceReducibleProviderErrors,
                ) {
                    Ok(ok) => {
                        successful_output_max_tokens = output_max_tokens;
                        let mut note = format!(
                            "Context was reduced after {} (prompt tokens {} -> {}).",
                            provider_failure_reason(&last_error),
                            original_prompt_tokens,
                            fallback.prompt_tokens
                        );
                        if fallback.trim_stats.diff_truncated {
                            note.push_str(
                                " Diff was truncated to fit the reduced provider-pressure budget.",
                            );
                        }
                        push_context_note(&mut review_context_note, note);
                        rendered = fallback;
                        result = Some(ok);
                        break;
                    }
                    Err(error) if snif_execution::is_reducible_provider_error(&error) => {
                        last_error = error;
                        last_output_max_tokens = output_max_tokens;
                        rendered = fallback;
                    }
                    Err(error) => return Err(error),
                }
            }

            match result {
                Some(result) => result,
                None => bail!(
                    "Review request failed after reduced-context retries due to {}. Last error: {}. Try lowering context.max_tokens and/or context.summarizer_concurrency.",
                    provider_failure_reason(&last_error),
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
    let initial_parsed = snif_output::parser::parse_response(&result.response)?;
    let mut parsed = initial_parsed.clone();
    let mut repaired_response = None;

    // Run repair if findings are empty OR if chain-of-thought leakage is detected
    let needs_repair =
        parsed.findings.is_empty() || snif_output::parser::has_chain_of_thought(&result.response);

    if needs_repair {
        tracing::warn!("Repairing review response");
        let repaired = snif_execution::repair_review_response_with_max_tokens(
            &result.response,
            &config.model,
            Some(successful_output_max_tokens),
        )?;
        parsed = snif_output::parser::parse_response(&repaired.response)?;
        repaired_response = Some(repaired.response);
    }

    let inconclusive_reason = if parsed.findings.is_empty() {
        snif_output::integrity::empty_review_inconclusive_reason(
            &result.response,
            &initial_parsed,
            repaired_response
                .as_deref()
                .map(|response| (response, &parsed)),
        )
    } else {
        None
    };
    let change_summary = parsed.summary;
    let mut findings = parsed.findings;

    // Apply static filters
    findings = snif_output::filter::apply_filters(findings, &config.filter);

    // Verify findings against source code — penalize evidence that doesn't
    // match the actual file content (catches LLM hallucinations)
    findings = snif_output::verify::verify_findings(findings, repo_path);

    // Re-apply confidence threshold after verification may have reduced scores
    findings.retain(|f| f.confidence >= config.filter.min_confidence);

    // Compute fingerprints
    snif_output::fingerprint::compute_fingerprints(&mut findings);

    if rendered.trim_stats != PromptTrimStats::default() && review_context_note.is_none() {
        push_context_note(
            &mut review_context_note,
            "Context was trimmed to fit the configured token budget.",
        );
    }

    let outcome = classify_review_outcome(
        inconclusive_reason,
        &findings,
        context_was_limited(&rendered, &review_context_note),
    );

    match outcome {
        ReviewOutcome::Inconclusive => {
            let reason = inconclusive_reason
                .map(|reason| reason.to_string())
                .unwrap_or_else(|| "review output could not be trusted".to_string());
            tracing::warn!(reason = %reason, "Review inconclusive");
            push_context_note(
                &mut review_context_note,
                format!("Review was inconclusive: {reason}."),
            );
        }
        ReviewOutcome::LimitedClean => {
            tracing::warn!("No reportable findings, but review context was limited");
        }
        ReviewOutcome::Clean => {
            tracing::info!("No findings — change looks clean");
        }
        ReviewOutcome::Findings => {
            tracing::info!(count = findings.len(), "Findings after filtering");
        }
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
                outcome,
                changed_paths: &changed_paths,
                retrieval_results: &retrieval_results,
                related_files_analyzed: rendered.related_files,
                diff_lines: diff.lines().count(),
                model_name: &config.model.review_model,
                duration_secs: result.duration.as_secs(),
                context_note: review_context_note.as_deref(),
            });
        adapter.post_summary(&summary)?;

        tracing::info!(posted = findings.len(), "Findings posted");

        if should_fail_inconclusive_review(outcome, config.review.inconclusive_mode) {
            bail_inconclusive(inconclusive_reason)?;
        }

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

    if should_fail_inconclusive_review(outcome, config.review.inconclusive_mode) {
        bail_inconclusive(inconclusive_reason)?;
    }

    Ok(())
}

fn push_context_note(note: &mut Option<String>, new_note: impl Into<String>) {
    let new_note = new_note.into();
    match note {
        Some(existing) => {
            existing.push(' ');
            existing.push_str(&new_note);
        }
        None => *note = Some(new_note),
    }
}

fn provider_failure_reason(error: &anyhow::Error) -> &'static str {
    if snif_execution::is_rate_limit_error(error) {
        "provider rate limiting"
    } else if snif_execution::is_provider_pressure_error(error) {
        "provider pressure"
    } else {
        "provider failure"
    }
}

fn classify_review_outcome(
    inconclusive_reason: Option<InconclusiveReason>,
    findings: &[snif_types::Finding],
    context_limited: bool,
) -> ReviewOutcome {
    if inconclusive_reason.is_some() {
        ReviewOutcome::Inconclusive
    } else if !findings.is_empty() {
        ReviewOutcome::Findings
    } else if context_limited {
        ReviewOutcome::LimitedClean
    } else {
        ReviewOutcome::Clean
    }
}

fn context_was_limited(
    rendered: &RenderedReviewPrompts,
    review_context_note: &Option<String>,
) -> bool {
    rendered.trim_stats != PromptTrimStats::default() || review_context_note.is_some()
}

fn bail_inconclusive(reason: Option<InconclusiveReason>) -> Result<()> {
    let reason = reason
        .map(|reason| reason.to_string())
        .unwrap_or_else(|| "review output could not be trusted".to_string());
    bail!("Review inconclusive: {reason}")
}

fn should_fail_inconclusive_review(outcome: ReviewOutcome, mode: ReviewInconclusiveMode) -> bool {
    outcome == ReviewOutcome::Inconclusive && mode == ReviewInconclusiveMode::Fail
}

fn render_prompts_for_prompt_budget(
    config: &snif_config::SnifConfig,
    context: &mut ContextPackage,
    prompt_budget: usize,
    reason: &str,
    allow_diff_truncation: bool,
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

        if allow_diff_truncation && !trim_stats.diff_truncated {
            let overflow = prompt_tokens.saturating_sub(prompt_budget);
            let current_diff_tokens = snif_context::budget::estimate_tokens(&context.diff);
            let target_diff_tokens = current_diff_tokens.saturating_sub(overflow + 256);

            if let Some(truncated_diff) =
                truncate_diff_to_token_budget(&context.diff, target_diff_tokens)
            {
                tracing::warn!(
                    tokens = prompt_tokens,
                    prompt_budget,
                    diff_tokens = current_diff_tokens,
                    target_diff_tokens,
                    reason,
                    "Truncating diff to fit reduced provider-pressure review budget"
                );
                context.diff = truncated_diff;
                trim_stats.diff_truncated = true;
                continue;
            }
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
    REVIEW_PROVIDER_PRESSURE_PROMPT_TARGETS
        .into_iter()
        .filter(move |target| *target < current_prompt_tokens)
}

fn review_output_max_tokens(config: &snif_config::SnifConfig, prompt_budget: usize) -> usize {
    config
        .context
        .output_reserve_tokens
        .max(1)
        .min(prompt_budget.max(1))
}

fn fallback_output_max_tokens(config: &snif_config::SnifConfig, prompt_budget: usize) -> usize {
    let reduced_cap = (prompt_budget / 4).max(REVIEW_PROVIDER_PRESSURE_MIN_OUTPUT_TOKENS);
    review_output_max_tokens(config, prompt_budget).min(reduced_cap)
}

fn truncate_diff_to_token_budget(diff: &str, token_budget: usize) -> Option<String> {
    let current_tokens = snif_context::budget::estimate_tokens(diff);
    if current_tokens <= token_budget {
        return None;
    }

    let notice_tokens = snif_context::budget::estimate_tokens(DIFF_TRUNCATION_NOTICE);
    let effective_budget = token_budget.saturating_sub(notice_tokens).max(1);
    let effective_bytes = effective_budget * context_constants::TOKENS_PER_CHAR_RATIO;
    let mut truncated = String::new();
    let mut used_bytes = 0_usize;

    for line in diff.lines() {
        let line_bytes = line.len() + 1;
        if used_bytes + line_bytes > effective_bytes {
            break;
        }
        truncated.push_str(line);
        truncated.push('\n');
        used_bytes += line_bytes;
    }

    if truncated.trim().is_empty() {
        truncated.push_str(diff.lines().next().unwrap_or_default());
        truncated.push('\n');
    }

    truncated.push_str(DIFF_TRUNCATION_NOTICE);
    Some(truncated)
}

fn fallback_reduces_request(
    previous_prompt_tokens: usize,
    previous_output_max_tokens: usize,
    next_prompt_tokens: usize,
    next_output_max_tokens: usize,
) -> bool {
    next_prompt_tokens < previous_prompt_tokens
        || next_output_max_tokens < previous_output_max_tokens
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

#[cfg(test)]
mod tests {
    use super::*;
    use snif_types::{BudgetReport, ChangeMetadata, ContextFile};

    #[test]
    fn fallback_targets_only_include_smaller_prompt_budgets() {
        assert_eq!(
            fallback_prompt_targets(91_184).collect::<Vec<_>>(),
            vec![64_000, 48_000, 32_000, 16_000, 8_000, 4_000]
        );
        assert_eq!(
            fallback_prompt_targets(50_000).collect::<Vec<_>>(),
            vec![48_000, 32_000, 16_000, 8_000, 4_000]
        );
        assert_eq!(
            fallback_prompt_targets(32_000).collect::<Vec<_>>(),
            vec![16_000, 8_000, 4_000]
        );
        assert_eq!(
            fallback_prompt_targets(37_129).collect::<Vec<_>>(),
            vec![32_000, 16_000, 8_000, 4_000]
        );
    }

    #[test]
    fn provider_failure_reason_distinguishes_rate_limit_from_pressure() {
        let rate_limit = anyhow::Error::new(snif_execution::LlmRetryFailure {
            kind: snif_execution::LlmRetryFailureKind::RateLimited,
            max_retries: 5,
            status: Some(429),
            retry_after: None,
            body: None,
            message: String::new(),
        });
        let pressure = anyhow::Error::new(snif_execution::LlmRetryFailure {
            kind: snif_execution::LlmRetryFailureKind::ProviderPressure,
            max_retries: 5,
            status: Some(504),
            retry_after: None,
            body: None,
            message: String::new(),
        });

        assert_eq!(
            provider_failure_reason(&rate_limit),
            "provider rate limiting"
        );
        assert_eq!(provider_failure_reason(&pressure), "provider pressure");
        assert!(snif_execution::is_reducible_provider_error(&rate_limit));
        assert!(snif_execution::is_reducible_provider_error(&pressure));
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
            render_prompts_for_prompt_budget(&config, &mut context, 8_000, "test trimming", false);

        assert!(rendered.trim_stats.related_files_removed > 0);
        assert_eq!(rendered.trim_stats.changed_files_degraded, 0);
        assert_eq!(context.changed_files[0].content_tier, ContentTier::Full);
    }

    #[test]
    fn review_output_cap_uses_reserve_but_shrinks_for_fallback_targets() {
        let config = snif_config::SnifConfig::default();

        assert_eq!(review_output_max_tokens(&config, 96_000), 32_000);
        assert_eq!(review_output_max_tokens(&config, 16_000), 16_000);
        assert_eq!(review_output_max_tokens(&config, 8_000), 8_000);
        assert_eq!(fallback_output_max_tokens(&config, 64_000), 16_000);
        assert_eq!(fallback_output_max_tokens(&config, 32_000), 8_000);
        assert_eq!(fallback_output_max_tokens(&config, 16_000), 4_000);
        assert_eq!(fallback_output_max_tokens(&config, 8_000), 2_000);
        assert_eq!(fallback_output_max_tokens(&config, 4_000), 1_024);
    }

    #[test]
    fn review_output_cap_clamps_zero_reserve() {
        let mut config = snif_config::SnifConfig::default();
        config.context.output_reserve_tokens = 0;

        assert_eq!(review_output_max_tokens(&config, 8_000), 1);
    }

    #[test]
    fn fallback_retry_can_reduce_output_cap_even_when_prompt_cannot_shrink() {
        assert!(fallback_reduces_request(15_596, 4_000, 15_596, 2_000));
        assert!(fallback_reduces_request(27_562, 8_000, 15_596, 4_000));
        assert!(!fallback_reduces_request(15_596, 2_000, 15_596, 2_000));
    }

    #[test]
    fn provider_pressure_fallback_can_truncate_irreducible_diff() {
        let config = snif_config::SnifConfig::default();
        let mut context = ContextPackage {
            metadata: ChangeMetadata::default(),
            diff: format!(
                "diff --git a/src/main.rs b/src/main.rs\n{}",
                "+let value = very_long_expression();\n".repeat(4_000)
            ),
            changed_files: Vec::new(),
            related_files: Vec::new(),
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
        let original_diff_len = context.diff.len();

        let rendered = render_prompts_for_prompt_budget(
            &config,
            &mut context,
            2_000,
            "provider pressure trimming",
            true,
        );

        assert!(rendered.trim_stats.diff_truncated);
        assert!(context.diff.len() < original_diff_len);
        assert!(context.diff.contains(DIFF_TRUNCATION_NOTICE.trim()));
    }

    #[test]
    fn context_notes_are_appended_in_order() {
        let mut note = None;

        push_context_note(&mut note, "Semantic retrieval was skipped.");
        push_context_note(&mut note, "Context was reduced.");

        assert_eq!(
            note.as_deref(),
            Some("Semantic retrieval was skipped. Context was reduced.")
        );
    }

    #[test]
    fn review_outcome_is_inconclusive_when_integrity_check_fails() {
        let outcome =
            classify_review_outcome(Some(InconclusiveReason::SummaryClaimsFindings), &[], false);

        assert_eq!(outcome, ReviewOutcome::Inconclusive);
    }

    #[test]
    fn review_outcome_marks_empty_limited_context_as_limited_clean() {
        let outcome = classify_review_outcome(None, &[], true);

        assert_eq!(outcome, ReviewOutcome::LimitedClean);
    }

    #[test]
    fn review_outcome_marks_empty_full_context_as_clean() {
        let outcome = classify_review_outcome(None, &[], false);

        assert_eq!(outcome, ReviewOutcome::Clean);
    }

    #[test]
    fn context_limited_tracks_prompt_trimming() {
        let rendered = RenderedReviewPrompts {
            system_prompt: String::new(),
            user_prompt: String::new(),
            prompt_tokens: 0,
            related_files: 0,
            trim_stats: PromptTrimStats {
                related_files_removed: 1,
                changed_files_degraded: 0,
                diff_truncated: false,
            },
        };

        assert!(context_was_limited(&rendered, &None));
    }

    #[test]
    fn inconclusive_fail_mode_exits_non_zero() {
        assert!(should_fail_inconclusive_review(
            ReviewOutcome::Inconclusive,
            ReviewInconclusiveMode::Fail
        ));
    }

    #[test]
    fn inconclusive_warn_mode_exits_zero() {
        assert!(!should_fail_inconclusive_review(
            ReviewOutcome::Inconclusive,
            ReviewInconclusiveMode::Warn
        ));
    }
}
