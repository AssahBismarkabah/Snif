use std::fmt;

use crate::parser::{ParseQuality, ParsedResponse};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InconclusiveReason {
    SummaryClaimsFindings,
    MalformedResponseContainsFindingLanguage,
    RepairDidNotProduceStructuredReview,
}

impl fmt::Display for InconclusiveReason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::SummaryClaimsFindings => {
                write!(f, "review summary claims issues but findings are empty")
            }
            Self::MalformedResponseContainsFindingLanguage => write!(
                f,
                "malformed review text appears to contain issues but no findings were parsed"
            ),
            Self::RepairDidNotProduceStructuredReview => {
                write!(f, "repair did not produce a structured review response")
            }
        }
    }
}

pub fn empty_review_inconclusive_reason(
    raw_response: &str,
    parsed: &ParsedResponse,
    repaired: Option<(&str, &ParsedResponse)>,
) -> Option<InconclusiveReason> {
    let final_parsed = repaired.map(|(_, parsed)| parsed).unwrap_or(parsed);
    let final_response = repaired
        .map(|(response, _)| response)
        .unwrap_or(raw_response);

    if !final_parsed.findings.is_empty() {
        return None;
    }

    if summary_claims_findings(&parsed.summary) || summary_claims_findings(&final_parsed.summary) {
        return Some(InconclusiveReason::SummaryClaimsFindings);
    }

    if let Some((_, repaired_parsed)) = repaired {
        if !matches!(
            repaired_parsed.quality,
            ParseQuality::Strict | ParseQuality::Flexible
        ) {
            return Some(InconclusiveReason::RepairDidNotProduceStructuredReview);
        }
    }

    if parsed.quality == ParseQuality::Malformed
        && (contains_finding_claim(raw_response) || contains_finding_claim(final_response))
    {
        return Some(InconclusiveReason::MalformedResponseContainsFindingLanguage);
    }

    None
}

pub fn summary_claims_findings(text: &str) -> bool {
    contains_finding_claim(text)
}

pub fn contains_finding_claim(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    let normalized = lower.split_whitespace().collect::<Vec<_>>().join(" ");

    if normalized.trim().is_empty() {
        return false;
    }

    const NEGATIVE_PATTERNS: &[&str] = &[
        "no issue",
        "no issues",
        "no bug",
        "no bugs",
        "no finding",
        "no findings",
        "no problem",
        "no problems",
        "no concern",
        "no concerns",
        "no reportable",
        "without issue",
        "without issues",
        "nothing actionable",
        "looks clean",
        "change looks clean",
        "clean change",
    ];

    if NEGATIVE_PATTERNS
        .iter()
        .any(|pattern| normalized.contains(pattern))
    {
        return false;
    }

    const CLAIM_PATTERNS: &[&str] = &[
        "issue found",
        "issues found",
        "found issue",
        "found an issue",
        "found a bug",
        "bug found",
        "security concern found",
        "security concerns found",
        "security concern identified",
        "security concerns identified",
        "security concern detected",
        "security concerns detected",
        "security concern flagged",
        "security concerns flagged",
        "concern found",
        "concerns found",
        "found concern",
        "found concerns",
        "vulnerability found",
        "vulnerabilities found",
        "found vulnerability",
        "found vulnerabilities",
        "regression found",
        "regressions found",
        "found regression",
        "found regressions",
        "risk found",
        "risks found",
        "found risk",
        "found risks",
        "problem found",
        "problems found",
        "found problem",
        "found problems",
        "concerns identified",
        "issues identified",
        "bugs identified",
        "vulnerabilities identified",
        "regressions identified",
        "risks identified",
        "problems identified",
        "identified concerns",
        "identified issues",
        "identified bugs",
        "identified vulnerabilities",
        "identified regressions",
        "identified risks",
        "identified problems",
        "detected concern",
        "detected concerns",
        "detected issue",
        "detected issues",
        "detected bug",
        "detected bugs",
        "detected vulnerability",
        "detected vulnerabilities",
        "flagged concern",
        "flagged concerns",
        "flagged issue",
        "flagged issues",
        "flagged bug",
        "flagged bugs",
        "flagged vulnerability",
        "flagged vulnerabilities",
    ];

    CLAIM_PATTERNS
        .iter()
        .any(|pattern| normalized.contains(pattern))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse_response;

    #[test]
    fn clean_empty_response_is_not_inconclusive() {
        let raw = r#"{"summary":"Clean change","findings":[]}"#;
        let parsed = parse_response(raw).expect("response should parse");

        assert_eq!(empty_review_inconclusive_reason(raw, &parsed, None), None);
    }

    #[test]
    fn issue_claiming_summary_with_empty_findings_is_inconclusive() {
        let raw =
            r#"{"summary":"Two security concerns identified in file handling.","findings":[]}"#;
        let parsed = parse_response(raw).expect("response should parse");

        assert_eq!(
            empty_review_inconclusive_reason(raw, &parsed, None),
            Some(InconclusiveReason::SummaryClaimsFindings)
        );
    }

    #[test]
    fn malformed_issue_language_with_empty_repair_is_inconclusive() {
        let raw = "Review of diff. Two security concerns identified: file reads are unsafe.";
        let parsed = parse_response(raw).expect("malformed response should still parse metadata");
        let repaired_raw = r#"{"summary":"","findings":[]}"#;
        let repaired = parse_response(repaired_raw).expect("repair should parse");

        assert_eq!(
            empty_review_inconclusive_reason(raw, &parsed, Some((repaired_raw, &repaired))),
            Some(InconclusiveReason::MalformedResponseContainsFindingLanguage)
        );
    }

    #[test]
    fn valid_finding_is_not_downgraded_by_summary_language() {
        let raw = r#"{
          "summary": "One bug identified.",
          "findings": [
            {
              "file": "src/lib.rs",
              "start_line": 10,
              "end_line": null,
              "category": "logic",
              "confidence": 0.9,
              "evidence": "panic!()",
              "explanation": "This can panic.",
              "impact": "The process can crash.",
              "suggestion": null
            }
          ]
        }"#;
        let parsed = parse_response(raw).expect("response should parse");

        assert_eq!(empty_review_inconclusive_reason(raw, &parsed, None), None);
    }

    #[test]
    fn no_issues_language_is_not_a_finding_claim() {
        assert!(!contains_finding_claim(
            "No issues found. Change looks clean."
        ));
        assert!(!contains_finding_claim(
            "No reportable issues found in analyzed context."
        ));
    }

    #[test]
    fn neutral_domain_words_are_not_finding_claims() {
        assert!(!contains_finding_claim("Risk scoring code was refactored."));
        assert!(!contains_finding_claim(
            "Security concern handling was moved into a helper."
        ));
    }

    #[test]
    fn empty_array_repair_is_not_a_structured_clean_review() {
        let raw = "Review of diff. No issues found.";
        let parsed = parse_response(raw).expect("malformed response should still parse metadata");
        let repaired_raw = "[]";
        let repaired = parse_response(repaired_raw).expect("array repair should parse");

        assert_eq!(
            empty_review_inconclusive_reason(raw, &parsed, Some((repaired_raw, &repaired))),
            Some(InconclusiveReason::RepairDidNotProduceStructuredReview)
        );
    }
}
