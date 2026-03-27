use anyhow::Result;
use serde::Deserialize;
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
            let mut findings = Vec::new();
            for line in json_str.lines() {
                let line = line.trim().trim_matches(',');
                if line.starts_with('{') {
                    if let Ok(finding) = serde_json::from_str::<Finding>(line) {
                        findings.push(finding);
                    }
                }
            }
            if findings.is_empty() {
                tracing::warn!("No findings could be parsed from response");
            }
            Ok(ParsedResponse {
                summary: String::new(),
                findings,
            })
        }
    }
}
