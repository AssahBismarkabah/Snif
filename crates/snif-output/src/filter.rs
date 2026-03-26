use snif_config::FilterConfig;
use snif_types::{Finding, FindingCategory};

pub fn apply_filters(findings: Vec<Finding>, config: &FilterConfig) -> Vec<Finding> {
    let before = findings.len();

    let filtered: Vec<Finding> = findings
        .into_iter()
        .filter(|f| {
            // Confidence check
            if f.confidence < config.min_confidence {
                tracing::debug!(
                    file = %f.location.path,
                    confidence = f.confidence,
                    "Filtered: below confidence threshold"
                );
                return false;
            }

            // Evidence check
            if f.evidence.trim().is_empty() {
                tracing::debug!(file = %f.location.path, "Filtered: empty evidence");
                return false;
            }

            // Impact check
            if f.impact.trim().is_empty() {
                tracing::debug!(file = %f.location.path, "Filtered: empty impact");
                return false;
            }

            // Style suppression
            if config.suppress_style_only && f.category == FindingCategory::Style {
                tracing::debug!(file = %f.location.path, "Filtered: style-only");
                return false;
            }

            true
        })
        .collect();

    // Deduplicate: keep highest confidence per location
    let deduped = deduplicate(filtered);

    let after = deduped.len();
    tracing::info!(before, after, filtered = before - after, "Findings filtered");

    deduped
}

fn deduplicate(findings: Vec<Finding>) -> Vec<Finding> {
    let mut seen: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    let mut result: Vec<Finding> = Vec::new();

    for finding in findings {
        let key = format!(
            "{}:{}:{}",
            finding.location.path, finding.location.start_line, finding.category
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
