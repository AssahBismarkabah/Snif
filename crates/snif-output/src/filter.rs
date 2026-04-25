use snif_config::constants::output_filter;
use snif_config::FilterConfig;
use snif_types::{Finding, FindingCategory};

/// Detects when the LLM is reasoning-to-dismiss rather than reporting a real issue.
///
/// Some models narrate their analysis in the `explanation` or `impact` fields,
/// including self-dismissal phrases like "no bug here" or "I will lower the confidence".
/// This catches those regardless of which model is configured.
fn is_self_dismissed(finding: &Finding) -> bool {
    let body = format!(
        "{} {}",
        finding.explanation.to_lowercase(),
        finding.impact.to_lowercase()
    );

    let dismissal_patterns = output_filter::DISMISSAL_PATTERNS;

    if dismissal_patterns.iter().any(|p| body.contains(p)) {
        return true;
    }

    // Detect "impact: none" or "impact: none," patterns (case-insensitive)
    let impact_none = output_filter::IMPACT_NONE_PATTERNS;
    if impact_none.iter().any(|p| body.contains(p)) {
        return true;
    }

    // "minimal security impact" without being a legitimate qualifier
    let minimal = output_filter::MINIMAL_IMPACT_PATTERNS;
    if body.contains(minimal[0]) || body.contains(minimal[1]) && !body.contains("minimal impact on")
    {
        return true;
    }

    false
}

pub fn apply_filters(findings: Vec<Finding>, config: &FilterConfig) -> Vec<Finding> {
    let before = findings.len();

    let filtered: Vec<Finding> = findings
        .into_iter()
        .filter(|f| {
            // Confidence check
            if f.confidence < config.min_confidence {
                tracing::debug!(
                    file = %f.location.file,
                    confidence = f.confidence,
                    "Filtered: below confidence threshold"
                );
                return false;
            }

            // Evidence check
            if f.evidence.trim().is_empty() {
                tracing::debug!(file = %f.location.file, "Filtered: empty evidence");
                return false;
            }

            // Impact check
            if f.impact.trim().is_empty() {
                tracing::debug!(file = %f.location.file, "Filtered: empty impact");
                return false;
            }

            // Filter out findings where the LLM reasoned itself out of the issue.
            // This is model-agnostic — catches any model that dumps chain-of-thought
            // into the finding fields.
            if is_self_dismissed(f) {
                tracing::debug!(
                    file = %f.location.file,
                    "Filtered: self-dismissed by LLM"
                );
                return false;
            }

            // Suppress style-only noise, but keep explicit convention findings.
            if config.suppress_style_only && matches!(f.category, FindingCategory::Style) {
                tracing::debug!(
                    file = %f.location.file,
                    category = %f.category,
                    "Filtered: style"
                );
                return false;
            }

            true
        })
        .collect();

    // Deduplicate: keep highest confidence per location
    let deduped = deduplicate(filtered);

    let after = deduped.len();
    tracing::info!(
        before,
        after,
        filtered = before - after,
        "Findings filtered"
    );

    deduped
}

fn deduplicate(findings: Vec<Finding>) -> Vec<Finding> {
    let mut seen: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    let mut result: Vec<Finding> = Vec::new();

    for finding in findings {
        let key = format!(
            "{}:{}:{}",
            finding.location.file, finding.location.start_line, finding.category
        );

        if let Some(&idx) = seen.get(&key) {
            if finding.confidence > result[idx].confidence {
                result[idx] = finding;
            }
        } else {
            seen.insert(key, result.len());
            result.push(finding);
        }
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEST_MIN_CONFIDENCE: f64 = 0.5;
    const TEST_CONFIDENCE_HIGH: f64 = 0.9;
    const TEST_CONFIDENCE_MID: f64 = 0.8;
    const TEST_LINE: usize = 10;
    const TEST_FILE: &str = "src/test.rs";
    const TEST_EVIDENCE: &str = "fn test() {}";

    fn default_config() -> FilterConfig {
        FilterConfig {
            min_confidence: TEST_MIN_CONFIDENCE,
            suppress_style_only: false,
            feedback_min_signals: 0,
        }
    }

    fn make_finding(explanation: &str, impact: &str, confidence: f64) -> Finding {
        Finding {
            location: snif_types::FileLocation {
                file: TEST_FILE.to_string(),
                start_line: TEST_LINE,
                end_line: None,
            },
            category: FindingCategory::Logic,
            confidence,
            evidence: TEST_EVIDENCE.to_string(),
            explanation: explanation.to_string(),
            impact: impact.to_string(),
            suggestion: None,
            fingerprint: None,
        }
    }

    #[test]
    fn self_dismissed_no_bug() {
        let f = make_finding("no bug here", "None", TEST_CONFIDENCE_HIGH);
        assert!(is_self_dismissed(&f));
    }

    #[test]
    fn self_dismissed_i_will_remove() {
        let f = make_finding(
            "I will remove this finding as it's speculative",
            "Minimal impact",
            TEST_CONFIDENCE_MID,
        );
        assert!(is_self_dismissed(&f));
    }

    #[test]
    fn self_dismissed_impact_none() {
        let f = make_finding("logic seems correct", "impact: none", TEST_CONFIDENCE_MID);
        assert!(is_self_dismissed(&f));
    }

    #[test]
    fn self_dismissed_acceptable_behavior() {
        let f = make_finding(
            "This is acceptable behavior for this use case",
            "No real issue",
            TEST_CONFIDENCE_MID,
        );
        assert!(is_self_dismissed(&f));
    }

    #[test]
    fn self_dismissed_i_will_look_for() {
        let f = make_finding(
            "I will look for a stronger bug",
            "Minor issue",
            TEST_CONFIDENCE_MID,
        );
        assert!(is_self_dismissed(&f));
    }

    #[test]
    fn self_dismissed_minimal_security_impact() {
        let f = make_finding(
            "Minor robustness issue, but not a critical security vulnerability",
            "Minimal security impact",
            TEST_CONFIDENCE_HIGH,
        );
        assert!(is_self_dismissed(&f));
    }

    #[test]
    fn legitimate_finding_passes() {
        let f = make_finding(
            "Concurrent writes to the history file can corrupt the JSONL format",
            "Data corruption in the history file if concurrent writes occur",
            0.95,
        );
        assert!(!is_self_dismissed(&f));
    }

    #[test]
    fn legitimate_performance_finding_passes() {
        let f = make_finding(
            "There is no locking mechanism. If multiple processes try to append simultaneously, writes could interleave.",
            "Concurrent writes can corrupt the JSONL file, leading to data loss",
            0.9,
        );
        assert!(!is_self_dismissed(&f));
    }

    #[test]
    fn filter_chain_removes_self_dismissed() {
        let findings = vec![
            make_finding("no bug here", "None", TEST_CONFIDENCE_HIGH), // should be filtered
            make_finding(
                "Concurrent writes corrupt JSONL",
                "Data corruption risk",
                0.95,
            ), // should pass
        ];
        let filtered = apply_filters(findings, &default_config());
        assert_eq!(filtered.len(), 1);
        assert!(filtered[0].explanation.contains("Concurrent"));
    }
}
