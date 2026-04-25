use sha2::{Digest, Sha256};
use snif_config::constants::{eval_output, output_filter};
use snif_types::{Finding, Fingerprint};
use std::collections::HashMap;

pub fn compute_fingerprints(findings: &mut [Finding]) {
    let mut content_counts: HashMap<String, usize> = HashMap::new();

    for finding in findings.iter_mut() {
        let content_hash = compute_content_hash(finding);
        let line_hash = compute_line_hash(finding);

        // Disambiguate when the same content hash appears multiple times
        let count = content_counts
            .entry(content_hash.clone())
            .or_insert(eval_output::DEFAULT_COUNTER);
        let id = if *count == eval_output::DEFAULT_COUNTER {
            content_hash.clone()
        } else {
            format!(
                "{}{}{}",
                content_hash,
                output_filter::FINGERPRINT_DISAMBIGUATION_SEPARATOR,
                count
            )
        };
        *count += 1;

        finding.fingerprint = Some(Fingerprint {
            id,
            line_id: line_hash,
        });
    }
}

/// Content-based hash: stable across rebases.
/// SHA256(file + category + normalize(evidence)) → 16 hex chars.
fn compute_content_hash(finding: &Finding) -> String {
    let mut hasher = Sha256::new();
    hasher.update(finding.location.file.as_bytes());
    hasher.update(finding.category.to_string().as_bytes());
    hasher.update(normalize_evidence(&finding.evidence).as_bytes());

    let hash = format!("{:x}", hasher.finalize());
    hash[..output_filter::FINGERPRINT_HASH_LENGTH].to_string()
}

/// Line-based hash: backward compatible with prior fingerprints.
/// SHA256(file + start_line + end_line + category) → 16 hex chars.
fn compute_line_hash(finding: &Finding) -> String {
    let mut hasher = Sha256::new();
    hasher.update(finding.location.file.as_bytes());
    hasher.update(finding.location.start_line.to_string().as_bytes());
    if let Some(end) = finding.location.end_line {
        hasher.update(end.to_string().as_bytes());
    }
    hasher.update(finding.category.to_string().as_bytes());

    let hash = format!("{:x}", hasher.finalize());
    hash[..output_filter::FINGERPRINT_HASH_LENGTH].to_string()
}

/// Normalize evidence text for stable hashing:
/// lowercase, collapse whitespace, trim.
fn normalize_evidence(evidence: &str) -> String {
    evidence
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}
