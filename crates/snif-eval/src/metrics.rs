use snif_config::constants::{eval_output, thresholds};
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

fn categories_match(actual: &str, expected: &str, acceptable: &[String]) -> bool {
    if actual == expected {
        return true;
    }
    // Fixture-level override: explicit list of acceptable alternatives
    if acceptable.iter().any(|c| c == actual) {
        return true;
    }
    // Global alias table: check both orderings
    eval_output::CATEGORY_ALIASES
        .iter()
        .any(|(a, b)| (actual == *a && expected == *b) || (actual == *b && expected == *a))
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

            let path_match = actual_finding.location.file == expected_finding.file;
            let category_match = categories_match(
                &actual_finding.category.to_string(),
                &expected_finding.category,
                &expected_finding.acceptable_categories,
            );
            let line_diff = (actual_finding.location.start_line as i64
                - expected_finding.start_line as i64)
                .unsigned_abs() as usize;
            let line_match = line_diff <= line_tolerance;

            if path_match && category_match && line_match {
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

    let precision = if total_tp + total_fp > eval_output::DEFAULT_COUNTER {
        total_tp as f64 / (total_tp + total_fp) as f64
    } else {
        eval_output::DEFAULT_PRECISION
    };

    let recall = if total_tp + total_fn > eval_output::DEFAULT_COUNTER {
        total_tp as f64 / (total_tp + total_fn) as f64
    } else {
        eval_output::DEFAULT_RECALL
    };

    let noise_rate = if total_actual > eval_output::DEFAULT_COUNTER {
        total_fp as f64 / total_actual as f64
    } else {
        eval_output::DEFAULT_NOISE_RATE
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
    let precision_ok = metrics.precision >= thresholds::EVAL_MIN_PRECISION;
    let recall_ok = metrics.recall >= thresholds::EVAL_MIN_RECALL;
    let noise_ok = metrics.noise_rate <= thresholds::EVAL_MAX_NOISE_RATE;

    if !precision_ok {
        let pct_precision = metrics.precision * eval_output::PERCENTAGE_MULTIPLIER;
        let threshold_pct = thresholds::EVAL_MIN_PRECISION * eval_output::PERCENTAGE_MULTIPLIER;
        tracing::error!(
            precision = format!("{:.1}%", pct_precision),
            "Quality gate FAILED: precision below {:.0}%",
            threshold_pct
        );
    }

    if !recall_ok {
        let pct_recall = metrics.recall * eval_output::PERCENTAGE_MULTIPLIER;
        let threshold_pct = thresholds::EVAL_MIN_RECALL * eval_output::PERCENTAGE_MULTIPLIER;
        tracing::error!(
            recall = format!("{:.1}%", pct_recall),
            "Quality gate FAILED: recall below {:.0}%",
            threshold_pct
        );
    }

    if !noise_ok {
        let pct_noise = metrics.noise_rate * eval_output::PERCENTAGE_MULTIPLIER;
        let threshold_pct = thresholds::EVAL_MAX_NOISE_RATE * eval_output::PERCENTAGE_MULTIPLIER;
        tracing::error!(
            noise_rate = format!("{:.1}%", pct_noise),
            "Quality gate FAILED: noise rate above {:.0}%",
            threshold_pct
        );
    }

    precision_ok && recall_ok && noise_ok
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixture::ExpectedFinding;
    use snif_types::{FileLocation, Finding, FindingCategory};

    const TEST_FIXTURE_NAME: &str = "test";
    const TEST_LINE_TOLERANCE: usize = 5;
    const TEST_CONFIDENCE_HIGH: f64 = 0.9;
    const TEST_EXPECTED_COUNT: usize = 1;
    const TEST_ZERO_COUNT: usize = 0;

    const TEST_FILE_PARSER: &str = "src/parser.ts";
    const TEST_FILE_INGEST: &str = "src/ingest.rs";
    const TEST_FILE_TRACKER: &str = "src/tracker.ts";
    const TEST_FILE_LIB: &str = "src/lib.rs";

    const TEST_LINE_PARSER: usize = 6;
    const TEST_LINE_INGEST: usize = 3;
    const TEST_LINE_TRACKER: usize = 5;
    const TEST_LINE_LIB: usize = 10;

    const TEST_CATEGORY_SECURITY: &str = "security";
    const TEST_CATEGORY_LOGIC: &str = "logic";
    const TEST_CATEGORY_PERFORMANCE: &str = "performance";
    const TEST_CATEGORY_OTHER: &str = "other";
    const TEST_CATEGORY_STYLE: &str = "style";
    const TEST_CATEGORY_CONVENTION: &str = "convention";

    fn make_finding(
        file: &str,
        line: usize,
        category: FindingCategory,
        confidence: f64,
    ) -> Finding {
        Finding {
            location: FileLocation {
                file: file.to_string(),
                start_line: line,
                end_line: None,
            },
            category,
            confidence,
            evidence: "evidence".to_string(),
            explanation: "explanation".to_string(),
            impact: "impact".to_string(),
            suggestion: None,
            fingerprint: None,
        }
    }

    #[test]
    fn category_alias_security_logic_match() {
        let expected = vec![ExpectedFinding {
            file: TEST_FILE_PARSER.to_string(),
            start_line: TEST_LINE_PARSER,
            category: TEST_CATEGORY_SECURITY.to_string(),
            acceptable_categories: vec![],
        }];
        let actual = vec![make_finding(
            TEST_FILE_PARSER,
            TEST_LINE_PARSER,
            FindingCategory::Logic,
            TEST_CONFIDENCE_HIGH,
        )];

        let result =
            compute_fixture_result(TEST_FIXTURE_NAME, &expected, &actual, TEST_LINE_TOLERANCE);
        assert_eq!(result.true_positives, TEST_EXPECTED_COUNT);
        assert_eq!(result.false_positives, TEST_ZERO_COUNT);
        assert_eq!(result.false_negatives, TEST_ZERO_COUNT);
    }

    #[test]
    fn category_alias_performance_logic_match() {
        let expected = vec![ExpectedFinding {
            file: TEST_FILE_INGEST.to_string(),
            start_line: TEST_LINE_INGEST,
            category: TEST_CATEGORY_SECURITY.to_string(),
            acceptable_categories: vec![],
        }];
        let actual = vec![make_finding(
            TEST_FILE_INGEST,
            TEST_LINE_INGEST,
            FindingCategory::Performance,
            TEST_CONFIDENCE_HIGH,
        )];

        let result =
            compute_fixture_result(TEST_FIXTURE_NAME, &expected, &actual, TEST_LINE_TOLERANCE);
        assert_eq!(result.true_positives, TEST_EXPECTED_COUNT);
        assert_eq!(result.false_positives, TEST_ZERO_COUNT);
    }

    #[test]
    fn fixture_acceptable_categories_override() {
        let expected = vec![ExpectedFinding {
            file: TEST_FILE_TRACKER.to_string(),
            start_line: TEST_LINE_TRACKER,
            category: TEST_CATEGORY_LOGIC.to_string(),
            acceptable_categories: vec![
                TEST_CATEGORY_PERFORMANCE.to_string(),
                TEST_CATEGORY_OTHER.to_string(),
            ],
        }];
        let actual = vec![make_finding(
            TEST_FILE_TRACKER,
            TEST_LINE_TRACKER,
            FindingCategory::Other,
            TEST_CONFIDENCE_HIGH,
        )];

        let result =
            compute_fixture_result(TEST_FIXTURE_NAME, &expected, &actual, TEST_LINE_TOLERANCE);
        assert_eq!(result.true_positives, TEST_EXPECTED_COUNT);
        assert_eq!(result.false_positives, TEST_ZERO_COUNT);
    }

    #[test]
    fn unrelated_category_still_mismatches() {
        let expected = vec![ExpectedFinding {
            file: TEST_FILE_LIB.to_string(),
            start_line: TEST_LINE_LIB,
            category: TEST_CATEGORY_SECURITY.to_string(),
            acceptable_categories: vec![],
        }];
        let actual = vec![make_finding(
            TEST_FILE_LIB,
            TEST_LINE_LIB,
            FindingCategory::Style,
            TEST_CONFIDENCE_HIGH,
        )];

        let result =
            compute_fixture_result(TEST_FIXTURE_NAME, &expected, &actual, TEST_LINE_TOLERANCE);
        assert_eq!(result.true_positives, TEST_ZERO_COUNT);
        assert_eq!(result.false_positives, TEST_EXPECTED_COUNT);
        assert_eq!(result.false_negatives, TEST_EXPECTED_COUNT);
    }

    #[test]
    fn categories_match_function_tests() {
        assert!(categories_match(
            TEST_CATEGORY_SECURITY,
            TEST_CATEGORY_LOGIC,
            &[]
        ));
        assert!(categories_match(
            TEST_CATEGORY_LOGIC,
            TEST_CATEGORY_SECURITY,
            &[]
        ));
        assert!(categories_match(
            TEST_CATEGORY_PERFORMANCE,
            TEST_CATEGORY_LOGIC,
            &[]
        ));
        assert!(categories_match(
            TEST_CATEGORY_OTHER,
            TEST_CATEGORY_SECURITY,
            &[]
        ));
        assert!(categories_match(
            TEST_CATEGORY_LOGIC,
            TEST_CATEGORY_LOGIC,
            &[]
        ));
        assert!(!categories_match(
            TEST_CATEGORY_STYLE,
            TEST_CATEGORY_SECURITY,
            &[]
        ));
        assert!(!categories_match(
            TEST_CATEGORY_CONVENTION,
            TEST_CATEGORY_SECURITY,
            &[]
        ));
        assert!(categories_match(
            TEST_CATEGORY_STYLE,
            TEST_CATEGORY_CONVENTION,
            &[]
        ));
    }
}
