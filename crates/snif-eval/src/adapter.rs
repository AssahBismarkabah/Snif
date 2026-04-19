use crate::history::EvalRecord;
use snif_config::constants::{eval, eval_thresholds};

/// Guidance text generated from analysis of past eval runs.
/// Appended to the system prompt to steer the model based on
/// observed precision, recall, and noise trends.
#[derive(Debug, Clone, Default)]
pub struct EvalGuidance {
    pub prompt_augmentation: String,
}

/// Helper to conditionally add guidance text
fn push_guidance_if(lines: &mut Vec<String>, condition: bool, text: &str) {
    if condition {
        lines.push(String::from(text));
    }
}

/// Analyzes the last N eval records and produces guidance text
/// that can be injected into the system prompt before the next run.
///
/// Strategy:
/// - Precision declining → tell the model to be more conservative
/// - Recall declining → tell the model to be more thorough on missed areas
/// - Noise rising → suppress weak categories
/// - Persistent per-fixture FP/FN patterns → targeted instructions
pub fn analyze_history(history: &[EvalRecord], window: usize) -> EvalGuidance {
    if history.is_empty() {
        return EvalGuidance::default();
    }

    let recent: Vec<&EvalRecord> = history.iter().rev().take(window).collect();
    if recent.len() < eval::MIN_RECORDS_FOR_TREND {
        // Not enough data for trend analysis; still check for persistent fixture patterns
        return analyze_fixture_patterns(&recent);
    }

    let mut lines: Vec<String> = Vec::new();

    // Trend analysis
    let precision_trend = compute_trend(&recent, |r| r.precision);
    let recall_trend = compute_trend(&recent, |r| r.recall);
    let noise_trend = compute_trend(&recent, |r| r.noise_rate);

    lines.push(String::from(eval::GUIDANCE_HEADER));
    push_guidance_if(&mut lines, precision_trend < eval_thresholds::PRECISION_DECLINE_THRESHOLD, eval::GUIDANCE_PRECISION_DECLINED);
    push_guidance_if(&mut lines, precision_trend > eval_thresholds::PRECISION_IMPROVEMENT_THRESHOLD, eval::GUIDANCE_PRECISION_STRONG);
    push_guidance_if(&mut lines, recall_trend < eval_thresholds::RECALL_DECLINE_THRESHOLD, eval::GUIDANCE_RECALL_DECLINED);
    push_guidance_if(&mut lines, recall_trend > eval_thresholds::RECALL_IMPROVEMENT_THRESHOLD, eval::GUIDANCE_RECALL_STRONG);
    push_guidance_if(&mut lines, noise_trend > eval_thresholds::NOISE_INCREASE_THRESHOLD, eval::GUIDANCE_NOISE_RISING);

    // Fixture-level pattern analysis
    let fixture_guidance = analyze_fixture_patterns(&recent);
    if !fixture_guidance.prompt_augmentation.is_empty() {
        lines.push(fixture_guidance.prompt_augmentation);
    }

    let mut guidance = EvalGuidance::default();
    if lines.len() > 1 {
        // More than just the header
        guidance.prompt_augmentation = lines.join("\n");
    }

    guidance
}

fn analyze_fixture_patterns(recent: &[&EvalRecord]) -> EvalGuidance {
    if recent.is_empty() {
        return EvalGuidance::default();
    }

    // Aggregate per-fixture stats across recent runs
    let mut fixture_fp_counts: std::collections::HashMap<String, usize> =
        std::collections::HashMap::new();
    let mut fixture_fn_counts: std::collections::HashMap<String, usize> =
        std::collections::HashMap::new();
    let mut fixture_runs: std::collections::HashMap<String, usize> =
        std::collections::HashMap::new();

    for record in recent {
        for fix in &record.per_fixture {
            let runs = fixture_runs.entry(fix.name.clone()).or_insert(0);
            *runs += 1;
            if fix.fp > 0 {
                let count = fixture_fp_counts.entry(fix.name.clone()).or_insert(0);
                *count += fix.fp;
            }
            if fix.fn_count > 0 {
                let count = fixture_fn_counts.entry(fix.name.clone()).or_insert(0);
                *count += fix.fn_count;
            }
        }
    }

    let mut lines: Vec<String> = Vec::new();

    // Find fixtures with persistent FP issues
    let persistent_fp = find_persistent_fixtures(&fixture_fp_counts, &fixture_runs);
    if !persistent_fp.is_empty() {
        let names: Vec<&str> = persistent_fp
            .iter()
            .map(|(n, _)| n.as_str())
            .take(eval_thresholds::MAX_FIXTURE_NAMES_IN_GUIDANCE)
            .collect();
        let suffix = if persistent_fp.len() > eval_thresholds::MAX_FIXTURE_NAMES_IN_GUIDANCE {
            format!(
                " and {} more",
                persistent_fp.len() - eval_thresholds::MAX_FIXTURE_NAMES_IN_GUIDANCE
            )
        } else {
            String::new()
        };
        lines.push(format!(
            "- The following fixtures have produced persistent false positives: {}{}. \
             These are likely clean or stylistic changes. Only report findings if you \
             identify a clear bug with concrete evidence.",
            names.join(", "),
            suffix,
        ));
    }

    // Find fixtures with persistent FN issues
    let persistent_fn = find_persistent_fixtures(&fixture_fn_counts, &fixture_runs);
    if !persistent_fn.is_empty() {
        let names: Vec<&str> = persistent_fn
            .iter()
            .map(|(n, _)| n.as_str())
            .take(eval_thresholds::MAX_FIXTURE_NAMES_IN_GUIDANCE)
            .collect();
        let suffix = if persistent_fn.len() > eval_thresholds::MAX_FIXTURE_NAMES_IN_GUIDANCE {
            format!(
                " and {} more",
                persistent_fn.len() - eval_thresholds::MAX_FIXTURE_NAMES_IN_GUIDANCE
            )
        } else {
            String::new()
        };
        lines.push(format!(
            "- The following fixtures have had findings missed in recent runs: {}{}. \
             Pay close attention to these patterns — they contain real bugs.",
            names.join(", "),
            suffix,
        ));
    }

    let mut guidance = EvalGuidance::default();
    if !lines.is_empty() {
        guidance.prompt_augmentation = lines.join("\n");
    }
    guidance
}

/// Helper to find fixtures with persistent issues (FP or FN)
fn find_persistent_fixtures<'a>(
    counts: &'a std::collections::HashMap<String, usize>,
    runs: &'a std::collections::HashMap<String, usize>,
) -> Vec<(&'a String, &'a usize)> {
    counts
        .iter()
        .filter(|(name, count)| {
            let run_count = runs.get(name.as_str()).copied().unwrap_or(1);
            run_count >= eval_thresholds::MIN_RUNS_FOR_PATTERN
                && **count as f64 / run_count as f64 > eval_thresholds::PERSISTENT_PATTERN_RATIO
        })
        .collect()
}

fn compute_trend(records: &[&EvalRecord], metric: fn(&EvalRecord) -> f64) -> f64 {
    if records.len() < 2 {
        return 0.0;
    }
    // `records` comes from `history.iter().rev().take(window)`,
    // so records[0] is the most recent, records[last] is the oldest.
    let newest = metric(records[0]);
    let oldest = metric(
        records
            .last()
            .expect("records.len() >= 2 guard ensures this exists"),
    );
    newest - oldest
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::history::FixtureRecord;

    // Test constants
    const TEST_WINDOW: usize = 5;
    const TEST_TIMESTAMP: &str = "2026-04-10T00:00:00Z";
    const TEST_GIT_SHA: &str = "test";

    // Baseline metric values
    const BASE_PRECISION: f64 = 0.90;
    const BASE_RECALL: f64 = 0.96;
    const BASE_NOISE: f64 = 0.10;

    // High precision values
    const HIGH_PRECISION: f64 = 0.95;

    // Low precision values
    const LOW_PRECISION: f64 = 0.84;
    const LOW_NOISE: f64 = 0.04;
    const HIGH_NOISE: f64 = 0.18;

    // Single record test values
    const SINGLE_RECORD_PRECISION: f64 = 0.80;
    const SINGLE_RECORD_RECALL: f64 = 0.90;
    const SINGLE_RECORD_NOISE: f64 = 0.10;

    // Fixture test data
    const FIXTURE_NAME: &str = "style-ts";
    const FIXTURE_EXPECTED: usize = 5;
    const FIXTURE_TP: usize = 5;

    // FP test scenario progression (3 runs with increasing FP counts)
    const FP_RUN_1_PRECISION: f64 = 0.90;
    const FP_RUN_1_NOISE: f64 = 0.10;
    const FP_RUN_1_ACTUAL: usize = 7;
    const FP_RUN_1_FP: usize = 2;

    const FP_RUN_2_PRECISION: f64 = 0.85;
    const FP_RUN_2_NOISE: f64 = 0.15;
    const FP_RUN_2_ACTUAL: usize = 8;
    const FP_RUN_2_FP: usize = 3;

    const FP_RUN_3_PRECISION: f64 = 0.82;
    const FP_RUN_3_NOISE: f64 = 0.18;
    const FP_RUN_3_ACTUAL: usize = 9;
    const FP_RUN_3_FP: usize = 4;

    fn make_record(
        precision: f64,
        recall: f64,
        noise: f64,
        fixtures: Vec<(&str, usize, usize, usize, usize)>,
    ) -> EvalRecord {
        EvalRecord {
            timestamp: TEST_TIMESTAMP.to_string(),
            git_sha: TEST_GIT_SHA.to_string(),
            fixture_count: fixtures.len(),
            precision,
            recall,
            noise_rate: noise,
            gates_passed: true,
            per_fixture: fixtures
                .into_iter()
                .map(|(name, expected, actual, tp, fp)| FixtureRecord {
                    name: name.to_string(),
                    expected,
                    actual,
                    tp,
                    fp,
                    fn_count: if tp == 0 && expected > 0 { 1 } else { 0 },
                })
                .collect(),
        }
    }

    #[test]
    fn empty_history_returns_empty_guidance() {
        let guidance = analyze_history(&[], TEST_WINDOW);
        assert!(guidance.prompt_augmentation.is_empty());
    }

    #[test]
    fn single_record_returns_no_trend_guidance() {
        let history = vec![make_record(SINGLE_RECORD_PRECISION, SINGLE_RECORD_RECALL, SINGLE_RECORD_NOISE, vec![])];
        let guidance = analyze_history(&history, TEST_WINDOW);
        // Single record, no trend possible; fixture patterns need 2+ runs
        assert!(guidance.prompt_augmentation.is_empty());
    }

    #[test]
    fn declining_precision_generates_conservative_guidance() {
        let history = vec![
            make_record(HIGH_PRECISION, BASE_RECALL, BASE_NOISE, vec![]),
            make_record(BASE_PRECISION, BASE_RECALL, BASE_NOISE, vec![]),
            make_record(LOW_PRECISION, BASE_RECALL, HIGH_NOISE, vec![]),
        ];
        let guidance = analyze_history(&history, TEST_WINDOW);
        assert!(
            guidance.prompt_augmentation.contains("more conservative"),
            "expected conservative guidance, got: {}",
            guidance.prompt_augmentation
        );
    }

    #[test]
    fn rising_noise_generates_suppression_guidance() {
        let history = vec![
            make_record(HIGH_PRECISION, BASE_RECALL, LOW_NOISE, vec![]),
            make_record(BASE_PRECISION, BASE_RECALL, BASE_NOISE, vec![]),
            make_record(LOW_PRECISION, BASE_RECALL, HIGH_NOISE, vec![]),
        ];
        let guidance = analyze_history(&history, TEST_WINDOW);
        assert!(
            guidance.prompt_augmentation.contains("Noise rate")
                || guidance.prompt_augmentation.contains("false positive"),
            "expected noise guidance, got: {}",
            guidance.prompt_augmentation
        );
    }

    #[test]
    fn persistent_fp_fixtures_generates_targeted_guidance() {
        // Need 3+ runs (MIN_RUNS_FOR_PATTERN) and >60% FP rate
        let history = vec![
            make_record(
                FP_RUN_1_PRECISION,
                BASE_RECALL,
                FP_RUN_1_NOISE,
                vec![(FIXTURE_NAME, FIXTURE_EXPECTED, FP_RUN_1_ACTUAL, FIXTURE_TP, FP_RUN_1_FP)],
            ),
            make_record(
                FP_RUN_2_PRECISION,
                BASE_RECALL,
                FP_RUN_2_NOISE,
                vec![(FIXTURE_NAME, FIXTURE_EXPECTED, FP_RUN_2_ACTUAL, FIXTURE_TP, FP_RUN_2_FP)],
            ),
            make_record(
                FP_RUN_3_PRECISION,
                BASE_RECALL,
                FP_RUN_3_NOISE,
                vec![(FIXTURE_NAME, FIXTURE_EXPECTED, FP_RUN_3_ACTUAL, FIXTURE_TP, FP_RUN_3_FP)],
            ),
        ];
        let guidance = analyze_history(&history, TEST_WINDOW);
        assert!(
            guidance
                .prompt_augmentation
                .contains("persistent false positive")
                || guidance.prompt_augmentation.contains("clean or stylistic"),
            "expected persistent-fixture guidance, got: {}",
            guidance.prompt_augmentation
        );
    }
}
