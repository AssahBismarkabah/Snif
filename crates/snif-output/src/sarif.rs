use serde_json::json;
use snif_types::Finding;

pub fn to_sarif(findings: &[Finding]) -> serde_json::Value {
    let rules: Vec<serde_json::Value> = vec![
        rule("snif/logic", "Logic Error", "Bug or incorrect behavior in code logic"),
        rule("snif/security", "Security Vulnerability", "Code that introduces a security risk"),
        rule("snif/convention", "Convention Violation", "Violation of project or language conventions"),
        rule("snif/performance", "Performance Issue", "Code that may cause performance degradation"),
        rule("snif/style", "Style Issue", "Code style or formatting concern"),
        rule("snif/other", "Other Issue", "Issue that does not fit other categories"),
    ];

    let results: Vec<serde_json::Value> = findings
        .iter()
        .map(|f| {
            let level = if f.confidence >= 0.9 {
                "error"
            } else if f.confidence >= 0.7 {
                "warning"
            } else {
                "note"
            };

            let rule_id = format!("snif/{}", f.category);

            let mut result = json!({
                "ruleId": rule_id,
                "level": level,
                "message": {
                    "text": f.explanation
                },
                "locations": [{
                    "physicalLocation": {
                        "artifactLocation": {
                            "uri": f.location.file
                        },
                        "region": {
                            "startLine": f.location.start_line
                        }
                    }
                }],
                "properties": {
                    "confidence": f.confidence,
                    "impact": f.impact,
                    "evidence": f.evidence
                }
            });

            if let Some(end) = f.location.end_line {
                result["locations"][0]["physicalLocation"]["region"]["endLine"] = json!(end);
            }

            if let Some(snippet) = &Some(&f.evidence) {
                result["locations"][0]["physicalLocation"]["region"]["snippet"] = json!({
                    "text": snippet
                });
            }

            if let Some(fp) = &f.fingerprint {
                result["partialFingerprints"] = json!({
                    "snif/v1": fp.id
                });
            }

            if let Some(suggestion) = &f.suggestion {
                result["properties"]["suggestion"] = json!(suggestion);
            }

            result
        })
        .collect();

    json!({
        "$schema": "https://raw.githubusercontent.com/oasis-tcs/sarif-spec/main/sarif-2.1/schema/sarif-schema-2.1.0.json",
        "version": "2.1.0",
        "runs": [{
            "tool": {
                "driver": {
                    "name": "snif",
                    "version": env!("CARGO_PKG_VERSION"),
                    "informationUri": "https://github.com/snif-project/snif",
                    "rules": rules
                }
            },
            "results": results
        }]
    })
}

fn rule(id: &str, name: &str, description: &str) -> serde_json::Value {
    json!({
        "id": id,
        "name": name,
        "shortDescription": {
            "text": description
        }
    })
}
