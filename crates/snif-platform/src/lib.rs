pub mod github;

use anyhow::Result;
use snif_types::{ChangeMetadata, Finding, Fingerprint};

pub trait PlatformAdapter {
    fn fetch_diff(&self) -> Result<String>;
    fn fetch_changed_paths(&self) -> Result<Vec<String>>;
    fn fetch_metadata(&self) -> Result<ChangeMetadata>;
    fn post_findings(&self, findings: &[Finding]) -> Result<()>;
    fn get_prior_fingerprints(&self) -> Result<Vec<Fingerprint>>;
    fn resolve_stale(&self, stale: &[Fingerprint]) -> Result<()>;
}

pub fn parse_changed_paths_from_diff(diff: &str) -> Vec<String> {
    let mut paths = Vec::new();
    for line in diff.lines() {
        if let Some(path) = line.strip_prefix("+++ b/") {
            if path != "/dev/null" {
                paths.push(path.to_string());
            }
        }
    }
    paths.sort();
    paths.dedup();
    paths
}

pub fn extract_identifiers_from_diff(diff: &str) -> Vec<String> {
    let mut identifiers = Vec::new();
    for line in diff.lines() {
        if line.starts_with('+') && !line.starts_with("+++") {
            let content = &line[1..];
            for word in content.split(|c: char| !c.is_alphanumeric() && c != '_') {
                let word = word.trim();
                if word.len() > 2 && word.chars().next().map_or(false, |c| c.is_alphabetic()) {
                    identifiers.push(word.to_string());
                }
            }
        }
    }
    identifiers.sort();
    identifiers.dedup();
    identifiers
}
