use anyhow::Result;
use std::path::Path;

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

    // Print results
    println!("\n=== Evaluation Results ===\n");
    for fr in &result.fixture_results {
        println!(
            "  {:<40} expected={} actual={} TP={} FP={} FN={}",
            fr.fixture_name,
            fr.expected,
            fr.actual,
            fr.true_positives,
            fr.false_positives,
            fr.false_negatives
        );
    }

    println!();
    println!("  Precision:   {:.1}%", result.aggregate.precision * 100.0);
    println!("  Recall:      {:.1}%", result.aggregate.recall * 100.0);
    println!("  Noise rate:  {:.1}%", result.aggregate.noise_rate * 100.0);
    println!();

    let record = snif_eval::history::build_record(
        &result.fixture_results,
        &result.aggregate,
        result.gates_passed,
    );

    if let Some(previous) = history.as_ref().and_then(|h| h.last()) {
        let warnings = snif_eval::history::check_regression(&record, previous);
        if !warnings.is_empty() {
            eprintln!("\n=== Regression Warnings ===\n");
            for warning in &warnings {
                eprintln!("WARNING: {}", warning.message);
            }
            eprintln!();
        }
    }

    snif_eval::history::save_record(history_path, &record)?;

    // Report to Braintrust monitoring if configured
    let braintrust_project_id = std::env::var("SNIF_BRAINTRUST_PROJECT_ID")
        .unwrap_or_else(|_| snif_eval::reporter::BRAINTRUST_DEFAULT_PROJECT_ID.to_string());
    if let Ok(api_key) = std::env::var("BRAINTRUST_API_KEY") {
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

    if result.gates_passed {
        println!("  Quality gates: PASSED");
    } else {
        println!("  Quality gates: FAILED");
        std::process::exit(1);
    }

    Ok(())
}
