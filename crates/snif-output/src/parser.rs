use anyhow::Result;
use snif_types::Finding;

pub fn parse_response(response: &str) -> Result<Vec<Finding>> {
    let trimmed = response.trim();

    // Try to find JSON array in the response
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
            tracing::info!(count = findings.len(), "Parsed findings from response");
            Ok(findings)
        }
        Err(e) => {
            tracing::warn!(error = %e, "Failed to parse findings JSON, attempting line-by-line");
            // Try to parse individual JSON objects from the response
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
            Ok(findings)
        }
    }
}
