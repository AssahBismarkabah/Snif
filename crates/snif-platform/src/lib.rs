pub mod github;
pub mod gitlab;

use anyhow::Result;
use snif_config::constants::platform::{
    BOT_MARKER, DIFF_HUNK_PREFIX, DIFF_NEW_PREFIX, DIFF_OLD_PREFIX, FINGERPRINT_END,
    FINGERPRINT_MARKER, LINE_FINGERPRINT_MARKER, NULL_PATH,
};
use snif_types::{ChangeMetadata, Finding, Fingerprint};

/// Formats a summary body with the bot marker prepended.
pub(crate) fn format_summary_body(summary: &str) -> String {
    format!("{}\n\n{}", BOT_MARKER, summary)
}

/// Formats a resolved finding body with the bot marker.
pub(crate) fn format_resolved_body() -> String {
    format!(
        "{}\n\n**Resolved** — this issue is no longer present in the current change.",
        BOT_MARKER
    )
}

/// Formats a unified diff header from old/new paths and content.
pub(crate) fn format_diff_header(old_path: &str, new_path: &str, diff: &str) -> String {
    format!(
        "{}{}\n{}{}\n{}\n",
        DIFF_OLD_PREFIX, old_path, DIFF_NEW_PREFIX, new_path, diff
    )
}

// Shared comment formatting used by all adapters
pub(crate) fn format_finding_body(finding: &Finding) -> String {
    let fingerprint_tags = finding
        .fingerprint
        .as_ref()
        .map(|fp| {
            format!(
                "{}{}{}\n{}{}{}",
                FINGERPRINT_MARKER,
                fp.id,
                FINGERPRINT_END,
                LINE_FINGERPRINT_MARKER,
                fp.line_id,
                FINGERPRINT_END
            )
        })
        .unwrap_or_default();

    format!(
        "{}\n{}\n\
         **[{}]** (confidence: {:.0}%)\n\n\
         {}\n\n\
         **Impact:** {}\n\n\
         **Evidence:**\n```\n{}\n```\
         {}\n",
        BOT_MARKER,
        fingerprint_tags,
        finding.category,
        finding.confidence * 100.0,
        finding.explanation,
        finding.impact,
        finding.evidence,
        finding
            .suggestion
            .as_ref()
            .map_or(String::new(), |s| format!("\n\n**Suggestion:** {}", s))
    )
}

/// Extract both fingerprint types from a comment body.
/// Returns (content_id, line_id). Either may be None for old comments.
pub(crate) fn extract_fingerprints(body: &str) -> (Option<String>, Option<String>) {
    let content_id = extract_marker_value(body, FINGERPRINT_MARKER);
    let line_id = extract_marker_value(body, LINE_FINGERPRINT_MARKER);
    (content_id, line_id)
}

fn extract_marker_value(body: &str, marker: &str) -> Option<String> {
    body.find(marker).and_then(|start| {
        let after = &body[start + marker.len()..];
        after.find(FINGERPRINT_END).map(|end| {
            let value = after[..end].trim().to_string();
            if value.is_empty() {
                return None;
            }
            Some(value)
        })?
    })
}

pub trait PlatformAdapter {
    fn fetch_diff(&self) -> Result<String>;
    fn fetch_changed_paths(&self) -> Result<Vec<String>>;
    fn fetch_metadata(&self) -> Result<ChangeMetadata>;
    fn post_findings(&self, findings: &[Finding]) -> Result<()>;
    fn post_summary(&self, summary: &str) -> Result<()>;
    fn get_prior_fingerprints(&self) -> Result<Vec<Fingerprint>>;
    fn resolve_stale(&self, stale: &[Fingerprint]) -> Result<()>;
}

pub fn parse_changed_paths_from_diff(diff: &str) -> Vec<String> {
    let mut paths = Vec::new();
    for line in diff.lines() {
        if let Some(path) = line.strip_prefix(DIFF_NEW_PREFIX) {
            if path != NULL_PATH {
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
        if line.starts_with('+') && !line.starts_with(DIFF_HUNK_PREFIX) {
            let content = &line[1..];
            for word in content.split(|c: char| !c.is_alphanumeric() && c != '_') {
                let word = word.trim();
                if word.len() > 2 && word.chars().next().is_some_and(|c| c.is_alphabetic()) {
                    identifiers.push(word.to_string());
                }
            }
        }
    }
    identifiers.sort();
    identifiers.dedup();
    identifiers
}
