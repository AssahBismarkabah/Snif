use anyhow::{Context, Result};
use snif_platform::PlatformAdapter;
use snif_types::ContentTier;
use std::path::Path;

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

    // Render prompts and enforce budget on the rendered output
    let output_reserve = config.context.output_reserve_tokens;
    let (system_prompt, user_prompt) = loop {
        let sys = snif_prompts::render_system_prompt(&config);
        let usr = snif_prompts::render_user_prompt(&context);
        let total_tokens = snif_context::budget::estimate_tokens(&sys)
            + snif_context::budget::estimate_tokens(&usr);

        if total_tokens + output_reserve <= config.context.max_tokens {
            break (sys, usr);
        }

        // Try removing related files first
        if context.related_files.pop().is_some() {
            tracing::warn!(
                tokens = total_tokens,
                budget = config.context.max_tokens,
                remaining_files = context.related_files.len(),
                "Prompt exceeds budget, trimming related file"
            );
            continue;
        }

        // Then degrade the largest Full-tier changed file to DiffOnly
        let largest_full = context
            .changed_files
            .iter()
            .enumerate()
            .filter(|(_, f)| f.content_tier == ContentTier::Full)
            .max_by_key(|(_, f)| f.content.len());

        if let Some((idx, _)) = largest_full {
            let file = &mut context.changed_files[idx];
            tracing::warn!(
                path = %file.path,
                content_bytes = file.content.len(),
                "Degrading changed file to diff-only to fit budget"
            );
            file.content = "[See diff for changes to this file.]".to_string();
            file.content_tier = ContentTier::DiffOnly;
            file.summary = None;
            continue;
        }

        // Nothing left to trim
        tracing::warn!(
            tokens = total_tokens,
            budget = config.context.max_tokens,
            "Prompt still exceeds budget after all degradation"
        );
        break (sys, usr);
    };

    tracing::info!(
        system_tokens = snif_context::budget::estimate_tokens(&system_prompt),
        user_tokens = snif_context::budget::estimate_tokens(&user_prompt),
        related_files = context.related_files.len(),
        "Prompts rendered"
    );

    // Execute review via LLM
    let result = snif_execution::execute_review(&system_prompt, &user_prompt, &config.model)?;

    tracing::info!(
        duration = ?result.duration,
        response_len = result.response.len(),
        "LLM execution complete"
    );

    // Parse findings from LLM response
    let parsed = snif_output::parser::parse_response(&result.response)?;
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
        "sarif" => {
            let sarif = snif_output::sarif::to_sarif(&findings);
            let sarif_json = serde_json::to_string_pretty(&sarif)?;
            std::fs::write("findings.sarif", &sarif_json)?;
            println!("{}", sarif_json);
        }
        _ => {
            if !findings.is_empty() {
                println!("{}", serde_json::to_string_pretty(&findings)?);
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
            });
        adapter.post_summary(&summary)?;

        tracing::info!(posted = findings.len(), "Findings posted");

        let current_ids: Vec<&str> = findings
            .iter()
            .filter_map(|f| f.fingerprint.as_ref().map(|fp| fp.id.as_str()))
            .collect();
        let stale: Vec<_> = prior
            .into_iter()
            .filter(|fp| !current_ids.contains(&fp.id.as_str()))
            .collect();

        if !stale.is_empty() {
            tracing::info!(count = stale.len(), "Resolving stale findings");
            adapter.resolve_stale(&stale)?;
        }
    }

    Ok(())
}

fn detect_platform(explicit: Option<&str>, config_default: &str) -> String {
    if let Some(p) = explicit {
        return p.to_string();
    }
    if let Ok(p) = std::env::var("SNIF_PLATFORM") {
        return p;
    }
    if std::env::var("CI_PROJECT_PATH").is_ok() {
        return "gitlab".to_string();
    }
    if std::env::var("GITHUB_REPOSITORY").is_ok() {
        return "github".to_string();
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
        "gitlab" => {
            let project_path = project
                .or(repo)
                .map(String::from)
                .or_else(|| std::env::var("CI_PROJECT_PATH").ok())
                .context(
                    "--project or CI_PROJECT_PATH required for GitLab. \
                     Make sure the pipeline runs with: rules: - if: $CI_PIPELINE_SOURCE == \"merge_request_event\"",
                )?;
            let mr_iid = pr
                .or_else(|| {
                    std::env::var("CI_MERGE_REQUEST_IID")
                        .ok()
                        .and_then(|s| s.parse().ok())
                })
                .context(
                    "--pr/--mr or CI_MERGE_REQUEST_IID required for GitLab. \
                     CI_MERGE_REQUEST_IID is only available in merge request pipelines. \
                     Add this rule to your .gitlab-ci.yml: rules: - if: $CI_PIPELINE_SOURCE == \"merge_request_event\"",
                )?;
            let api_base = config_api_base
                .map(String::from)
                .or_else(|| std::env::var("CI_API_V4_URL").ok());
            Ok(Box::new(snif_platform::gitlab::GitLabAdapter::new(
                &project_path,
                mr_iid,
                api_base.as_deref(),
            )?))
        }
        _ => {
            let repo_str = repo
                .map(String::from)
                .or_else(|| std::env::var("GITHUB_REPOSITORY").ok())
                .context("--repo or GITHUB_REPOSITORY required for GitHub")?;
            let pr_num = pr
                .or_else(|| {
                    std::env::var("SNIF_PR_NUMBER")
                        .ok()
                        .and_then(|s| s.parse().ok())
                })
                .context("--pr or SNIF_PR_NUMBER required for GitHub")?;
            let parts: Vec<&str> = repo_str.splitn(2, '/').collect();
            if parts.len() != 2 {
                anyhow::bail!("--repo must be in owner/repo format");
            }
            Ok(Box::new(snif_platform::github::GitHubAdapter::new(
                parts[0], parts[1], pr_num,
            )?))
        }
    }
}
