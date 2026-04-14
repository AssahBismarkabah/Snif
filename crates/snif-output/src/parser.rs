use anyhow::Result;
use serde::Deserialize;
use serde_json::Value;
use snif_types::Finding;

#[derive(Debug, Deserialize)]
struct ReviewResponse {
    summary: Option<String>,
    findings: Vec<Finding>,
}

pub struct ParsedResponse {
    pub summary: String,
    pub findings: Vec<Finding>,
}

/// Chain-of-thought patterns that indicate leaked reasoning.
const COT_PATTERNS: &[&str] = &[
    "let me think",
    "let's look",
    "let me analyze",
    "actually,",
    "wait,",
    "however, there is",
    "however, the",
    "i need to",
    "i should",
    "i will",
    "the code slices",
    "the real issue",
    "more critically",
    "more significantly",
    "let's look closer",
    "looking at",
    "examining",
    "checking",
    "the most concrete",
    "the most significant",
    "a more significant",
    "a potential panic",
    "potential panic",
    "this is likely",
    "this is technically",
    "while this is",
    "if this is",
    "if the",
    "if git_sha",
    "if record",
    "but the",
    "but if",
    "so the",
    "so no",
    "so it",
    "wait,",
    "actually",
];

/// Extract the outermost balanced JSON object from a response that may contain
/// chain-of-thought preamble text.
fn extract_json_object(input: &str) -> Option<&str> {
    let start = input.find('{')?;
    let after_open = &input[start..];

    let mut depth = 0usize;
    let mut in_string = false;
    let mut escape = false;
    let mut end_pos = None;

    for (i, ch) in after_open.char_indices() {
        if in_string {
            if escape {
                escape = false;
            } else if ch == '\\' {
                escape = true;
            } else if ch == '"' {
                in_string = false;
            }
            continue;
        }

        match ch {
            '"' => in_string = true,
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    end_pos = Some(i);
                    break;
                }
            }
            _ => {}
        }
    }

    end_pos.map(|end| &after_open[..=end])
}

/// Check if the response contains chain-of-thought leakage patterns.
fn contains_cot_patterns(response: &str) -> bool {
    let lower = response.to_lowercase();
    COT_PATTERNS.iter().any(|pattern| lower.contains(pattern))
}

/// Sanitize a finding's text fields by removing chain-of-thought preamble.
/// Strips common reasoning patterns from the beginning of the text and keeps
/// only the concrete issue description.
fn sanitize_text(text: &str) -> String {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return String::new();
    }

    let lower = trimmed.to_lowercase();

    // If the text starts with a COT pattern, try to find where the concrete
    // statement begins by looking for sentence boundaries after COT markers.
    if COT_PATTERNS
        .iter()
        .any(|p| lower.starts_with(p) || lower.contains(p))
    {
        // Try to find the first sentence that doesn't start with COT patterns
        let sentences: Vec<&str> = trimmed.split(&['.', '!', '?', '\n']).collect();
        for sentence in &sentences {
            let s = sentence.trim();
            if s.is_empty() {
                continue;
            }
            let s_lower = s.to_lowercase();
            if !COT_PATTERNS.iter().any(|p| s_lower.contains(p)) {
                return format!(
                    "{}.",
                    s.trim_start_matches(|c: char| !c.is_alphabetic()).trim()
                );
            }
        }
        // If all sentences contain COT patterns, return the last part after the
        // last COT marker
        for pattern in COT_PATTERNS {
            if let Some(pos) = lower.find(pattern) {
                let after = &trimmed[pos + pattern.len()..];
                // Skip past any punctuation/spaces after the pattern
                let clean = after
                    .trim_start_matches(|c: char| !c.is_alphabetic())
                    .trim();
                if !clean.is_empty() && clean.len() > 5 {
                    // Only return if there's substantial content
                    return clean.to_string();
                }
            }
        }
        // Fall back to returning the original text if nothing clean was found
        trimmed.to_string()
    } else {
        trimmed.to_string()
    }
}

/// Sanitize all text fields in findings to remove chain-of-thought leakage.
fn sanitize_findings(findings: &mut [Finding]) {
    for finding in findings {
        finding.explanation = sanitize_text(&finding.explanation);
        finding.impact = sanitize_text(&finding.impact);
        finding.evidence = sanitize_text(&finding.evidence);
        if let Some(ref mut suggestion) = finding.suggestion {
            *suggestion = sanitize_text(suggestion);
        }
    }
}

pub fn parse_response(response: &str) -> Result<ParsedResponse> {
    let trimmed = response.trim();

    // Extract clean JSON object, stripping any chain-of-thought preamble
    let json_str = if let Some(extracted) = extract_json_object(trimmed) {
        extracted
    } else {
        trimmed
    };

    // Try to parse as the new object format: { "summary": "...", "findings": [...] }
    if let Ok(review) = serde_json::from_str::<ReviewResponse>(json_str) {
        let summary = review.summary.unwrap_or_default();
        let mut findings = review.findings;
        sanitize_findings(&mut findings);
        tracing::info!(
            count = findings.len(),
            has_summary = !summary.is_empty(),
            "Parsed review response"
        );
        return Ok(ParsedResponse { summary, findings });
    }

    if let Ok(value) = serde_json::from_str::<Value>(json_str) {
        if let Some(mut parsed) = parsed_response_from_value(&value) {
            sanitize_findings(&mut parsed.findings);
            tracing::info!(
                count = parsed.findings.len(),
                has_summary = !parsed.summary.is_empty(),
                "Parsed review response with flexible object extraction"
            );
            return Ok(parsed);
        }
    }

    // Fall back to parsing as a plain JSON array (backwards compatibility)
    let json_str = if let Some(start) = json_str.find('[') {
        if let Some(end) = json_str.rfind(']') {
            &json_str[start..=end]
        } else {
            json_str
        }
    } else {
        json_str
    };

    match serde_json::from_str::<Vec<Finding>>(json_str) {
        Ok(mut findings) => {
            sanitize_findings(&mut findings);
            tracing::info!(count = findings.len(), "Parsed findings (array format)");
            Ok(ParsedResponse {
                summary: String::new(),
                findings,
            })
        }
        Err(e) => {
            tracing::warn!(error = %e, "Failed to parse response, attempting line-by-line");
            let mut findings = salvage_findings(trimmed);
            sanitize_findings(&mut findings);
            if findings.is_empty() {
                tracing::warn!("No findings could be parsed from response");
            } else {
                tracing::info!(
                    count = findings.len(),
                    "Recovered findings from malformed response"
                );
            }
            Ok(ParsedResponse {
                summary: String::new(),
                findings,
            })
        }
    }
}

/// Check if the raw response contains chain-of-thought patterns that warrant repair.
pub fn has_chain_of_thought(response: &str) -> bool {
    contains_cot_patterns(response)
}

fn parsed_response_from_value(value: &Value) -> Option<ParsedResponse> {
    match value {
        Value::Object(map) => {
            let summary = map
                .get("summary")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string();

            for key in ["findings", "issues", "results"] {
                if let Some(Value::Array(items)) = map.get(key) {
                    let findings = parse_findings(items);
                    if !findings.is_empty() || !summary.is_empty() {
                        return Some(ParsedResponse { summary, findings });
                    }
                }
            }

            for value in map.values() {
                if let Some(parsed) = parsed_response_from_value(value) {
                    return Some(ParsedResponse {
                        summary: if summary.is_empty() {
                            parsed.summary
                        } else {
                            summary
                        },
                        findings: parsed.findings,
                    });
                }
            }

            None
        }
        Value::Array(items) => {
            let findings = parse_findings(items);
            if findings.is_empty() {
                None
            } else {
                Some(ParsedResponse {
                    summary: String::new(),
                    findings,
                })
            }
        }
        _ => None,
    }
}

fn parse_findings(items: &[Value]) -> Vec<Finding> {
    items
        .iter()
        .filter_map(|item| serde_json::from_value::<Finding>(item.clone()).ok())
        .collect()
}

fn salvage_findings(response: &str) -> Vec<Finding> {
    let object_region = response
        .find("\"findings\"")
        .and_then(|index| response[index..].find('[').map(|offset| index + offset))
        .map(|index| &response[index..])
        .unwrap_or(response);

    let mut findings = Vec::new();
    for object in extract_balanced_objects(object_region) {
        if let Ok(finding) = serde_json::from_str::<Finding>(&object) {
            findings.push(finding);
        }
    }

    findings
}

fn extract_balanced_objects(input: &str) -> Vec<String> {
    let mut objects = Vec::new();
    let mut start = None;
    let mut depth = 0usize;
    let mut in_string = false;
    let mut escape = false;

    for (index, ch) in input.char_indices() {
        if in_string {
            if escape {
                escape = false;
            } else if ch == '\\' {
                escape = true;
            } else if ch == '"' {
                in_string = false;
            }
            continue;
        }

        match ch {
            '"' => in_string = true,
            '{' => {
                if depth == 0 {
                    start = Some(index);
                }
                depth += 1;
            }
            '}' => {
                if depth == 0 {
                    continue;
                }

                depth -= 1;
                if depth == 0 {
                    if let Some(start_index) = start.take() {
                        objects.push(input[start_index..=index].to_string());
                    }
                }
            }
            _ => {}
        }
    }

    objects
}

#[cfg(test)]
mod tests {
    use super::{has_chain_of_thought, parse_response};

    #[test]
    fn parses_object_with_issues_field() {
        let response = r#"{
          "summary": "Found a bug.",
          "issues": [
            {
              "file": "src/lib.rs",
              "start_line": 7,
              "end_line": 8,
              "category": "logic",
              "confidence": 0.9,
              "evidence": "panic!()",
              "explanation": "This panics.",
              "impact": "The process crashes.",
              "suggestion": null
            }
          ]
        }"#;

        let parsed = parse_response(response).unwrap();
        assert_eq!(parsed.findings.len(), 1);
        assert_eq!(parsed.summary, "Found a bug.");
    }

    #[test]
    fn recovers_complete_finding_from_truncated_response() {
        let response = r#"{
          "summary": "Found a bug.",
          "findings": [
            {
              "file": "src/lib.rs",
              "start_line": 7,
              "end_line": 8,
              "category": "logic",
              "confidence": 0.9,
              "evidence": "panic!()",
              "explanation": "This panics.",
              "impact": "The process crashes.",
              "suggestion": null
            },
            {
              "file": "src/other.rs",
              "start_line": 12,
              "category": "security",
              "confidence": 1.0,
              "evidence": "unterminated
        "#;

        let parsed = parse_response(response).unwrap();
        assert_eq!(parsed.findings.len(), 1);
        assert_eq!(parsed.findings[0].location.file, "src/lib.rs");
    }

    #[test]
    fn detects_chain_of_thought_in_response() {
        let response = "Let me think about this code. {\"summary\":\"test\",\"findings\":[]}";
        assert!(has_chain_of_thought(response));
    }

    #[test]
    fn does_not_detect_clean_response_as_cot() {
        let response = r#"{"summary":"Clean change","findings":[]}"#;
        assert!(!has_chain_of_thought(response));
    }

    #[test]
    fn sanitizes_cot_from_finding_explanation() {
        let response = r#"Let me think about this.
        {
          "summary": "Found issue",
          "findings": [
            {
              "file": "src/lib.rs",
              "start_line": 7,
              "end_line": 8,
              "category": "logic",
              "confidence": 0.9,
              "evidence": "panic!()",
              "explanation": "Let me analyze this. Actually, the code panics here.",
              "impact": "The process crashes.",
              "suggestion": null
            }
          ]
        }"#;

        let parsed = parse_response(response).unwrap();
        assert_eq!(parsed.findings.len(), 1);
        // The explanation should be sanitized - "Let me analyze" and "Actually" should be stripped
        let explanation = &parsed.findings[0].explanation;
        assert!(!explanation.to_lowercase().contains("let me analyze"));
        assert!(!explanation.to_lowercase().starts_with("actually"));
    }
}
