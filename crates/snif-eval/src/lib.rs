pub mod adapter;
pub mod fixture;
pub mod history;
pub mod metrics;
pub mod reporter;
pub mod retry;

use anyhow::Result;
use snif_config::constants::{eval, eval_output, model, thresholds};
use snif_config::SnifConfig;
use snif_types::{BudgetReport, ChangeMetadata, ContentTier, ContextFile, ContextPackage, Finding};
use std::path::Path;

pub struct EvalResult {
    pub fixture_results: Vec<metrics::FixtureResult>,
    pub aggregate: metrics::AggregateMetrics,
    pub gates_passed: bool,
}

pub fn run_evaluation(
    fixtures_path: &Path,
    config: &SnifConfig,
    history: Option<&[history::EvalRecord]>,
) -> Result<EvalResult> {
    let fixtures = fixture::load_fixtures(fixtures_path)?;

    if fixtures.is_empty() {
        anyhow::bail!("No fixtures found in {}", fixtures_path.display());
    }

    // Generate guidance from past eval history
    let guidance = history
        .map(|h| adapter::analyze_history(h, eval::HISTORY_WINDOW))
        .filter(|g| !g.prompt_augmentation.is_empty());

    if let Some(ref g) = guidance {
        tracing::info!(
            guidance_len = g.prompt_augmentation.len(),
            "Eval guidance generated from history"
        );
    }

    let mut fixture_results = Vec::new();

    for fix in &fixtures {
        tracing::info!(name = %fix.name, "Running fixture");

        let changed_files: Vec<ContextFile> = fix
            .files
            .iter()
            .map(|(path, content)| ContextFile {
                path: path.clone(),
                content: content.clone(),
                summary: None,
                retrieval_score: None,
                content_tier: ContentTier::Full,
            })
            .collect();

        let context = ContextPackage {
            metadata: ChangeMetadata::default(),
            diff: fix.diff.clone(),
            changed_files,
            related_files: vec![],
            omissions: vec![],
            budget: BudgetReport {
                total_budget: model::DEFAULT_MAX_TOKENS,
                diff_tokens: eval_output::DEFAULT_TOKEN_COUNT,
                changed_files_tokens: eval_output::DEFAULT_TOKEN_COUNT,
                related_files_tokens: eval_output::DEFAULT_TOKEN_COUNT,
                remaining_tokens: model::DEFAULT_MAX_TOKENS,
                files_included: fix.files.len(),
                files_omitted: eval_output::DEFAULT_FILE_COUNT,
                files_full: fix.files.len(),
                files_summary_only: eval_output::DEFAULT_FILE_COUNT,
                files_diff_only: eval_output::DEFAULT_FILE_COUNT,
            },
        };

        let guidance_text = guidance.as_ref().map(|g| g.prompt_augmentation.as_str());
        let system_prompt = snif_prompts::render_system_prompt_with_conventions(
            config,
            fix.conventions.as_deref(),
            guidance_text,
        );
        let user_prompt = snif_prompts::render_user_prompt(&context);

        let mut all_runs = Vec::new();
        for attempt in 1..=fix.retry_count {
            tracing::info!(
                fixture = %fix.name,
                attempt,
                total = fix.retry_count,
                "Running fixture attempt"
            );
            let findings =
                execute_fixture_attempt(&system_prompt, &user_prompt, config, &fix.name)?;
            log_findings(&fix.name, &findings);
            all_runs.push(findings);
        }

        let findings = if fix.retry_count == 1 {
            all_runs.pop().unwrap_or_default()
        } else {
            let retry_count = fix.retry_count as usize;
            let threshold = retry_count.div_ceil(2);
            retry::aggregate_findings(&all_runs, threshold, thresholds::EVAL_LINE_TOLERANCE)
        };

        let fixture_result = metrics::compute_fixture_result(
            &fix.name,
            &fix.expected_findings,
            &findings,
            thresholds::EVAL_LINE_TOLERANCE,
            fix.retry_count,
        );

        tracing::info!(
            name = %fix.name,
            expected = fixture_result.expected,
            actual = fixture_result.actual,
            tp = fixture_result.true_positives,
            fp = fixture_result.false_positives,
            retry_count = fix.retry_count,
            "Fixture complete"
        );

        fixture_results.push(fixture_result);
    }

    let aggregate = metrics::aggregate(&fixture_results);
    let gates_passed = metrics::check_quality_gates(&aggregate);

    tracing::info!(
        precision = format!("{:.1}%", aggregate.precision * 100.0),
        recall = format!("{:.1}%", aggregate.recall * 100.0),
        noise = format!("{:.1}%", aggregate.noise_rate * 100.0),
        gates = if gates_passed { "PASSED" } else { "FAILED" },
        "Evaluation complete"
    );

    Ok(EvalResult {
        fixture_results,
        aggregate,
        gates_passed,
    })
}

fn execute_fixture_attempt(
    system_prompt: &str,
    user_prompt: &str,
    config: &SnifConfig,
    fixture_name: &str,
) -> Result<Vec<Finding>> {
    let result = snif_execution::execute_review(system_prompt, user_prompt, &config.model)?;

    let mut parsed = snif_output::parser::parse_response(&result.response)?;

    let needs_repair =
        parsed.findings.is_empty() || snif_output::parser::has_chain_of_thought(&result.response);

    if needs_repair {
        tracing::warn!(fixture = %fixture_name, "Repairing review response");
        let repaired = snif_execution::repair_review_response(&result.response, &config.model)?;
        parsed = snif_output::parser::parse_response(&repaired.response)?;
    }

    Ok(snif_output::filter::apply_filters(
        parsed.findings,
        &config.filter,
    ))
}

fn log_findings(fixture_name: &str, findings: &[Finding]) {
    for f in findings {
        tracing::info!(
            fixture = %fixture_name,
            file = %f.location.file,
            line = f.location.start_line,
            category = %f.category,
            confidence = f.confidence,
            explanation = %f.explanation,
            "Finding"
        );
    }
}
