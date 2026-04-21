use anyhow::Result;
use snif_config::env::{app, keys};
use std::path::Path;

fn print_section_header(title: &str) {
    println!();
    println!("=== {} ===", title);
    println!();
}

fn print_fixture_result(
    name: &str,
    expected: usize,
    actual: usize,
    tp: usize,
    fp: usize,
    fn_count: usize,
) {
    println!(
        "  {:<40} expected={} actual={} TP={} FP={} FN={}",
        name, expected, actual, tp, fp, fn_count
    );
}

fn print_metric(label: &str, value: f64) {
    println!("  {:<12} {:.1}%", label, value);
}

fn print_warning(message: &str) {
    eprintln!("WARNING: {}", message);
}

fn print_quality_gates(passed: bool) {
    println!(
        "  Quality gates: {}",
        if passed { "PASSED" } else { "FAILED" }
    );
}

pub fn run(path: &str, fixtures: &str, history: &str) -> Result<()> {
    let repo_path = Path::new(path);
    let fixtures_path = Path::new(fixtures);
    let history_path = Path::new(history);

    tracing::info!(fixtures = %fixtures_path.display(), "Starting evaluation");

    let config = snif_config::SnifConfig::load(repo_path)?;

    // Load past eval history for feedback-driven guidance
    let history = snif_eval::history::load_history(history_path)
        .inspect_err(|e| {
            tracing::warn!(error = %e, "Failed to load eval history — running without guidance");
        })
        .ok();
    let history_refs = history.as_deref();

    let result = snif_eval::run_evaluation(fixtures_path, &config, history_refs)?;

    // Display results
    print_section_header("Evaluation Results");

    for fr in &result.fixture_results {
        print_fixture_result(
            &fr.fixture_name,
            fr.expected,
            fr.actual,
            fr.true_positives,
            fr.false_positives,
            fr.false_negatives,
        );
    }

    println!();
    print_metric("Precision", result.aggregate.precision * 100.0);
    print_metric("Recall", result.aggregate.recall * 100.0);
    print_metric("Noise rate", result.aggregate.noise_rate * 100.0);
    println!();

    let record = snif_eval::history::build_record(
        &result.fixture_results,
        &result.aggregate,
        result.gates_passed,
    );

    if let Some(previous) = history.as_ref().and_then(|h| h.last()) {
        let warnings = snif_eval::history::check_regression(&record, previous);
        if !warnings.is_empty() {
            print_section_header("Regression Warnings");
            for warning in &warnings {
                print_warning(&warning.message);
            }
            println!();
        }
    }

    snif_eval::history::save_record(history_path, &record)?;

    // Report to Braintrust monitoring if configured
    let braintrust_project_id = std::env::var(app::SNIF_BRAINTRUST_PROJECT_ID)
        .ok()
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| snif_config::constants::braintrust::DEFAULT_PROJECT_ID.to_string());
    if let Ok(api_key) = std::env::var(keys::BRAINTRUST_API_KEY) {
        let model_name = &config.model.review_model;
        match snif_eval::reporter::report_to_braintrust(
            &api_key,
            &braintrust_project_id,
            model_name,
            &record,
            &result.fixture_results,
        ) {
            Ok(_) => tracing::info!("Results reported to Braintrust"),
            Err(e) => tracing::warn!(error = %e, "Failed to report to Braintrust"),
        }
    }

    print_quality_gates(result.gates_passed);
    if !result.gates_passed {
        std::process::exit(1);
    }

    Ok(())
}
