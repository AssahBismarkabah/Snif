pub mod fixture;
pub mod metrics;

use anyhow::Result;
use snif_config::SnifConfig;
use snif_types::{BudgetReport, ChangeMetadata, ContentTier, ContextFile, ContextPackage};
use std::path::Path;

pub struct EvalResult {
    pub fixture_results: Vec<metrics::FixtureResult>,
    pub aggregate: metrics::AggregateMetrics,
    pub gates_passed: bool,
}

pub fn run_evaluation(fixtures_path: &Path, config: &SnifConfig) -> Result<EvalResult> {
    let fixtures = fixture::load_fixtures(fixtures_path)?;

    if fixtures.is_empty() {
        anyhow::bail!("No fixtures found in {}", fixtures_path.display());
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
                total_budget: 128_000,
                diff_tokens: 0,
                changed_files_tokens: 0,
                related_files_tokens: 0,
                remaining_tokens: 128_000,
                files_included: fix.files.len(),
                files_omitted: 0,
                files_full: fix.files.len(),
                files_summary_only: 0,
                files_diff_only: 0,
            },
        };

        let system_prompt = snif_prompts::render_system_prompt_with_conventions(
            config,
            fix.conventions.as_deref(),
        );
        let user_prompt = snif_prompts::render_user_prompt(&context);

        let result = snif_execution::execute_review(&system_prompt, &user_prompt, &config.model)?;

        let parsed = snif_output::parser::parse_response(&result.response)?;
        let mut findings = parsed.findings;
        findings = snif_output::filter::apply_filters(findings, &config.filter);

        for f in &findings {
            tracing::info!(
                fixture = %fix.name,
                file = %f.location.file,
                line = f.location.start_line,
                category = %f.category,
                confidence = f.confidence,
                explanation = %f.explanation,
                "Finding"
            );
        }

        let fixture_result =
            metrics::compute_fixture_result(&fix.name, &fix.expected_findings, &findings, 5);

        tracing::info!(
            name = %fix.name,
            expected = fixture_result.expected,
            actual = fixture_result.actual,
            tp = fixture_result.true_positives,
            fp = fixture_result.false_positives,
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
