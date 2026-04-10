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

pub fn parse_response(response: &str) -> Result<ParsedResponse> {
    let trimmed = response.trim();

    // Try to parse as the new object format: { "summary": "...", "findings": [...] }
    if let Some(start) = trimmed.find('{') {
        if let Some(end) = trimmed.rfind('}') {
            let json_str = &trimmed[start..=end];
            if let Ok(review) = serde_json::from_str::<ReviewResponse>(json_str) {
                let summary = review.summary.unwrap_or_default();
                tracing::info!(
                    count = review.findings.len(),
                    has_summary = !summary.is_empty(),
                    "Parsed review response"
                );
                return Ok(ParsedResponse {
                    summary,
                    findings: review.findings,
                });
            }

            if let Ok(value) = serde_json::from_str::<Value>(json_str) {
                if let Some(parsed) = parsed_response_from_value(&value) {
                    tracing::info!(
                        count = parsed.findings.len(),
                        has_summary = !parsed.summary.is_empty(),
                        "Parsed review response with flexible object extraction"
                    );
                    return Ok(parsed);
                }
            }
        }
    }

    // Fall back to parsing as a plain JSON array (backwards compatibility)
    let json_str = if let Some(start) = trimmed.find('[') {
        if let Some(end) = trimmed.rfind(']') {
            &trimmed[start..=end]
        } else {
            trimmed
        }
    } else {
        trimmed
    };

    match serde_json::from_str::<Vec<Finding>>(json_str) {
        Ok(findings) => {
            tracing::info!(count = findings.len(), "Parsed findings (array format)");
            Ok(ParsedResponse {
                summary: String::new(),
                findings,
            })
        }
        Err(e) => {
            tracing::warn!(error = %e, "Failed to parse response, attempting line-by-line");
            let findings = salvage_findings(trimmed);
            if findings.is_empty() {
                tracing::warn!("No findings could be parsed from response");
            } else {
                tracing::info!(count = findings.len(), "Recovered findings from malformed response");
            }
            Ok(ParsedResponse {
                summary: String::new(),
                findings,
            })
        }
    }
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
    items.iter()
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
    use super::parse_response;

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
}
