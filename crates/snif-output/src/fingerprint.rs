use sha2::{Digest, Sha256};
use snif_types::{Finding, Fingerprint};

pub fn compute_fingerprints(findings: &mut [Finding]) {
    for finding in findings.iter_mut() {
        let fingerprint = compute_fingerprint(finding);
        finding.fingerprint = Some(fingerprint);
    }
}

fn compute_fingerprint(finding: &Finding) -> Fingerprint {
    let mut hasher = Sha256::new();
    hasher.update(finding.location.file.as_bytes());
    hasher.update(finding.location.start_line.to_string().as_bytes());
    if let Some(end) = finding.location.end_line {
        hasher.update(end.to_string().as_bytes());
    }
    hasher.update(finding.category.to_string().as_bytes());

    let hash = format!("{:x}", hasher.finalize());
    // Use first 16 chars for a compact fingerprint
    Fingerprint {
        id: hash[..16].to_string(),
    }
}
