use anyhow::Result;
use std::path::Path;

pub fn run(path: &str, fixtures: &str, history: &str) -> Result<()> {
    let repo_path = Path::new(path);
    let fixtures_path = Path::new(fixtures);
    let history_path = Path::new(history);

    tracing::info!(fixtures = %fixtures_path.display(), "Starting evaluation");

    let config = snif_config::SnifConfig::load(repo_path)?;

    let result = snif_eval::run_evaluation(fixtures_path, &config)?;

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
    let previous_records = snif_eval::history::load_history(history_path)?;

    if let Some(previous) = previous_records.last() {
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

    if result.gates_passed {
        println!("  Quality gates: PASSED");
    } else {
        println!("  Quality gates: FAILED");
        std::process::exit(1);
    }

    Ok(())
}
