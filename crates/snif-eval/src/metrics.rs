use snif_types::Finding;

use crate::fixture::ExpectedFinding;

pub struct FixtureResult {
    pub fixture_name: String,
    pub expected: usize,
    pub actual: usize,
    pub true_positives: usize,
    pub false_positives: usize,
    pub false_negatives: usize,
}

pub struct AggregateMetrics {
    pub total_fixtures: usize,
    pub total_expected: usize,
    pub total_actual: usize,
    pub total_tp: usize,
    pub total_fp: usize,
    pub total_fn: usize,
    pub precision: f64,
    pub recall: f64,
    pub noise_rate: f64,
}

pub fn compute_fixture_result(
    fixture_name: &str,
    expected: &[ExpectedFinding],
    actual: &[Finding],
    line_tolerance: usize,
) -> FixtureResult {
    let mut matched_expected = vec![false; expected.len()];
    let mut matched_actual = vec![false; actual.len()];

    for (ai, actual_finding) in actual.iter().enumerate() {
        for (ei, expected_finding) in expected.iter().enumerate() {
            if matched_expected[ei] {
                continue;
            }

            let path_match = actual_finding.location.path == expected_finding.file;
            let line_diff =
                (actual_finding.location.start_line as i64 - expected_finding.start_line as i64).unsigned_abs() as usize;
            let line_match = line_diff <= line_tolerance;

            if path_match && line_match {
                matched_expected[ei] = true;
                matched_actual[ai] = true;
                break;
            }
        }
    }

    let true_positives = matched_actual.iter().filter(|&&m| m).count();
    let false_positives = matched_actual.iter().filter(|&&m| !m).count();
    let false_negatives = matched_expected.iter().filter(|&&m| !m).count();

    FixtureResult {
        fixture_name: fixture_name.to_string(),
        expected: expected.len(),
        actual: actual.len(),
        true_positives,
        false_positives,
        false_negatives,
    }
}

pub fn aggregate(results: &[FixtureResult]) -> AggregateMetrics {
    let total_tp: usize = results.iter().map(|r| r.true_positives).sum();
    let total_fp: usize = results.iter().map(|r| r.false_positives).sum();
    let total_fn: usize = results.iter().map(|r| r.false_negatives).sum();
    let total_expected: usize = results.iter().map(|r| r.expected).sum();
    let total_actual: usize = results.iter().map(|r| r.actual).sum();

    let precision = if total_tp + total_fp > 0 {
        total_tp as f64 / (total_tp + total_fp) as f64
    } else {
        1.0
    };

    let recall = if total_tp + total_fn > 0 {
        total_tp as f64 / (total_tp + total_fn) as f64
    } else {
        1.0
    };

    let noise_rate = if total_actual > 0 {
        total_fp as f64 / total_actual as f64
    } else {
        0.0
    };

    AggregateMetrics {
        total_fixtures: results.len(),
        total_expected,
        total_actual,
        total_tp,
        total_fp,
        total_fn,
        precision,
        recall,
        noise_rate,
    }
}

pub fn check_quality_gates(metrics: &AggregateMetrics) -> bool {
    let precision_ok = metrics.precision >= 0.70;
    let noise_ok = metrics.noise_rate <= 0.20;

    if !precision_ok {
        tracing::error!(
            precision = format!("{:.1}%", metrics.precision * 100.0),
            "Quality gate FAILED: precision below 70%"
        );
    }

    if !noise_ok {
        tracing::error!(
            noise_rate = format!("{:.1}%", metrics.noise_rate * 100.0),
            "Quality gate FAILED: noise rate above 20%"
        );
    }

    precision_ok && noise_ok
}
