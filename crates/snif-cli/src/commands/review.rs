use anyhow::{Context, Result};
use snif_platform::PlatformAdapter;
use std::path::Path;

pub fn run(
    path: &str,
    repo: Option<&str>,
    pr: Option<u64>,
    diff_file: Option<&str>,
    format: &str,
) -> Result<()> {
    let repo_path = Path::new(path);
    tracing::info!(path = %repo_path.display(), "Starting review");

    let config = snif_config::SnifConfig::load(repo_path)?;
    let store = snif_store::Store::open(Path::new(&config.index.db_path))?;

    // Get the diff — either from a file, GitHub API, or fail
    let (diff, metadata, changed_paths) = if let Some(diff_path) = diff_file {
        let diff = std::fs::read_to_string(diff_path)
            .context("Failed to read diff file")?;
        let paths = snif_platform::parse_changed_paths_from_diff(&diff);
        let metadata = snif_types::ChangeMetadata::default();
        (diff, metadata, paths)
    } else if let (Some(repo_str), Some(pr_num)) = (repo, pr) {
        let parts: Vec<&str> = repo_str.splitn(2, '/').collect();
        if parts.len() != 2 {
            anyhow::bail!("--repo must be in owner/repo format");
        }
        let adapter = snif_platform::github::GitHubAdapter::new(parts[0], parts[1], pr_num)?;
        let diff = adapter.fetch_diff()?;
        let metadata = adapter.fetch_metadata()?;
        let paths = adapter.fetch_changed_paths()?;
        (diff, metadata, paths)
    } else {
        anyhow::bail!(
            "Provide either --diff-file <path> or --repo <owner/repo> --pr <number>"
        );
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
    let context = snif_context::build_context(
        &diff,
        &changed_paths,
        &retrieval_results,
        repo_path,
        &store,
        &config.context,
        metadata,
    )?;

    // Render prompts
    let system_prompt = snif_prompts::render_system_prompt(&config);
    let user_prompt = snif_prompts::render_user_prompt(&context);

    tracing::info!(
        system_len = system_prompt.len(),
        user_len = user_prompt.len(),
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
    let mut findings = snif_output::parser::parse_response(&result.response)?;

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
            println!("{}", serde_json::to_string_pretty(&sarif)?);
        }
        _ => {
            if !findings.is_empty() {
                println!("{}", serde_json::to_string_pretty(&findings)?);
            }
        }
    }

    // If GitHub adapter is available, manage annotation lifecycle
    if let (Some(repo_str), Some(pr_num)) = (repo, pr) {
        let parts: Vec<&str> = repo_str.splitn(2, '/').collect();
        if parts.len() == 2 {
            let adapter = snif_platform::github::GitHubAdapter::new(parts[0], parts[1], pr_num)?;

            // Fetch prior findings for lifecycle management
            let prior = adapter.get_prior_fingerprints()?;

            // Post new findings
            adapter.post_findings(&findings)?;
            tracing::info!(posted = findings.len(), "Findings posted to GitHub PR");

            // Resolve stale findings (present before, absent now)
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
    }

    Ok(())
}
