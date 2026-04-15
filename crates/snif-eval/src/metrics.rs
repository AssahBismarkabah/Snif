use snif_types::Finding;

use crate::fixture::ExpectedFinding;

/// Minimum acceptable precision for the quality gate.
const MIN_PRECISION: f64 = 0.70;

/// Maximum acceptable noise rate (false positive ratio) for the quality gate.
const MAX_NOISE_RATE: f64 = 0.20;

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

/// Categories that are semantically close enough to count as a match.
/// When the model returns one of these categories for a finding whose
/// expected category is the paired one (or vice versa), we still count
/// it as a true positive.  This prevents the double-penalty problem
/// (TP=0, FP=1, FN=1) when the bug genuinely sits on a category boundary.
const CATEGORY_ALIASES: &[(&str, &str)] = &[
    ("security", "logic"),
    ("performance", "logic"),
    ("performance", "security"),
    ("convention", "style"),
    ("other", "logic"),
    ("other", "security"),
    ("other", "performance"),
];

fn categories_match(actual: &str, expected: &str, acceptable: &[String]) -> bool {
    if actual == expected {
        return true;
    }
    // Fixture-level override: explicit list of acceptable alternatives
    if acceptable.iter().any(|c| c == actual) {
        return true;
    }
    // Global alias table: check both orderings
    CATEGORY_ALIASES
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
    let precision_ok = metrics.precision >= MIN_PRECISION;
    let noise_ok = metrics.noise_rate <= MAX_NOISE_RATE;

    if !precision_ok {
        tracing::error!(
            precision = format!("{:.1}%", metrics.precision * 100.0),
            "Quality gate FAILED: precision below {:.0}%",
            MIN_PRECISION * 100.0
        );
    }

    if !noise_ok {
        tracing::error!(
            noise_rate = format!("{:.1}%", metrics.noise_rate * 100.0),
            "Quality gate FAILED: noise rate above {:.0}%",
            MAX_NOISE_RATE * 100.0
        );
    }

    precision_ok && noise_ok
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixture::ExpectedFinding;
    use snif_types::{FileLocation, Finding, FindingCategory};

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
            file: "src/parser.ts".to_string(),
            start_line: 6,
            category: "security".to_string(),
            acceptable_categories: vec![],
        }];
        let actual = vec![make_finding(
            "src/parser.ts",
            6,
            FindingCategory::Logic,
            0.9,
        )];

        let result = compute_fixture_result("test", &expected, &actual, 5);
        assert_eq!(result.true_positives, 1);
        assert_eq!(result.false_positives, 0);
        assert_eq!(result.false_negatives, 0);
    }

    #[test]
    fn category_alias_performance_logic_match() {
        let expected = vec![ExpectedFinding {
            file: "src/ingest.rs".to_string(),
            start_line: 3,
            category: "security".to_string(),
            acceptable_categories: vec![],
        }];
        let actual = vec![make_finding(
            "src/ingest.rs",
            3,
            FindingCategory::Performance,
            0.9,
        )];

        let result = compute_fixture_result("test", &expected, &actual, 5);
        assert_eq!(result.true_positives, 1);
        assert_eq!(result.false_positives, 0);
    }

    #[test]
    fn fixture_acceptable_categories_override() {
        let expected = vec![ExpectedFinding {
            file: "src/tracker.ts".to_string(),
            start_line: 5,
            category: "logic".to_string(),
            acceptable_categories: vec!["performance".to_string(), "other".to_string()],
        }];
        let actual = vec![make_finding(
            "src/tracker.ts",
            5,
            FindingCategory::Other,
            0.9,
        )];

        let result = compute_fixture_result("test", &expected, &actual, 5);
        assert_eq!(result.true_positives, 1);
        assert_eq!(result.false_positives, 0);
    }

    #[test]
    fn unrelated_category_still_mismatches() {
        let expected = vec![ExpectedFinding {
            file: "src/lib.rs".to_string(),
            start_line: 10,
            category: "security".to_string(),
            acceptable_categories: vec![],
        }];
        let actual = vec![make_finding("src/lib.rs", 10, FindingCategory::Style, 0.9)];

        let result = compute_fixture_result("test", &expected, &actual, 5);
        assert_eq!(result.true_positives, 0);
        assert_eq!(result.false_positives, 1);
        assert_eq!(result.false_negatives, 1);
    }

    #[test]
    fn categories_match_function_tests() {
        assert!(categories_match("security", "logic", &[]));
        assert!(categories_match("logic", "security", &[]));
        assert!(categories_match("performance", "logic", &[]));
        assert!(categories_match("other", "security", &[]));
        assert!(categories_match("logic", "logic", &[]));
        assert!(!categories_match("style", "security", &[]));
        assert!(!categories_match("convention", "security", &[]));
        assert!(categories_match("style", "convention", &[]));
    }
}
