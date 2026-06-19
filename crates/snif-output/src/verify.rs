use std::path::Path;

use snif_types::Finding;

/// Confidence penalty applied when a finding's evidence cannot be verified
/// against the actual file content. 0.3 reduces a 0.95 confidence to 0.65,
/// which falls below the default 0.7 minimum and gets filtered out.
const EVIDENCE_MISMATCH_PENALTY: f64 = 0.3;

/// Confidence penalty when the referenced file doesn't exist at all.
/// Less severe than a mismatch — the file could have been deleted in the diff.
const FILE_NOT_FOUND_PENALTY: f64 = 0.2;

/// Verify that the evidence cited in each finding actually appears in the
/// referenced source file. Findings with unverifiable evidence are given a
/// reduced confidence score, which may cause them to be filtered out by the
/// minimum confidence threshold.
///
/// This catches LLM hallucinations where the model fabricates function names,
/// code snippets, or line references that don't exist in the codebase.
pub fn verify_findings(findings: Vec<Finding>, repo_root: &Path) -> Vec<Finding> {
    let before = findings.len();
    let verified: Vec<Finding> = findings
        .into_iter()
        .map(|finding| verify_finding(finding, repo_root))
        .collect();
    let penalized = verified
        .iter()
        .filter(|f| f.confidence < EVIDENCE_MISMATCH_PENALTY + 0.01)
        .count();
    tracing::info!(
        before,
        after = verified.len(),
        penalized,
        "Findings verified against source"
    );
    verified
}

fn verify_finding(finding: Finding, repo_root: &Path) -> Finding {
    let file_path = repo_root.join(&finding.location.file);

    let file_content = match std::fs::read_to_string(&file_path) {
        Ok(content) => content,
        Err(_) => {
            tracing::debug!(
                file = %finding.location.file,
                "Finding references file that doesn't exist, applying penalty"
            );
            return Finding {
                confidence: (finding.confidence - FILE_NOT_FOUND_PENALTY).max(0.0),
                ..finding
            };
        }
    };

    if is_evidence_found(&finding.evidence, &file_content) {
        return finding;
    }

    // Evidence not found in file — apply penalty
    tracing::debug!(
        file = %finding.location.file,
        evidence_preview = truncate(&finding.evidence, 80),
        "Evidence not found in source file, applying penalty"
    );
    Finding {
        confidence: (finding.confidence - EVIDENCE_MISMATCH_PENALTY).max(0.0),
        ..finding
    }
}

/// Check whether any substantive code snippet from the evidence appears in the
/// file content. Extracts potential code identifiers and quoted snippets from
/// the evidence field and searches for them in the source.
fn is_evidence_found(evidence: &str, file_content: &str) -> bool {
    let candidates = extract_code_candidates(evidence);

    if candidates.is_empty() {
        // No extractable code references — can't verify, assume correct
        return true;
    }

    // If ANY candidate is found in the file, the evidence is verified.
    // The LLM may describe surrounding context inaccurately while still
    // referencing the real code, so a single match is sufficient.
    candidates
        .iter()
        .any(|candidate| file_content.contains(candidate))
}

/// Extract code identifiers and quoted snippets from the evidence field.
/// Focuses on likely-real code references: function names, method calls,
/// variable names, and quoted code snippets.
fn extract_code_candidates(evidence: &str) -> Vec<String> {
    let mut candidates = Vec::new();

    // Extract code from backtick-delimited spans (the LLM often quotes code
    // in backticks, and the quoted code is the most reliable reference)
    let mut in_backtick = false;
    let mut current = String::new();
    for ch in evidence.chars() {
        if ch == '`' {
            if in_backtick {
                let trimmed = current.trim().to_string();
                if trimmed.len() >= 2 {
                    candidates.push(trimmed);
                }
                current.clear();
            }
            in_backtick = !in_backtick;
        } else if in_backtick {
            current.push(ch);
        }
    }

    // Extract Rust identifiers from the evidence — function names, method calls,
    // and variable names that appear as snake_case or camelCase identifiers.
    // These catch references like `delete_summaries_for_files` or `Store`
    let common_words = [
        "the", "for", "and", "not", "but", "has", "can", "will", "may", "its", "are", "all", "was",
        "this", "that", "with", "from", "when", "then", "than", "also", "into", "does", "only",
        "The", "This", "That", "These", "Those", "When", "Then", "Each", "Every", "Both", "Either",
        "Neither", "Such",
    ];

    for word in evidence.split(|c: char| {
        c.is_whitespace()
            || c == ','
            || c == '.'
            || c == ';'
            || c == ':'
            || c == '('
            || c == ')'
            || c == '['
            || c == ']'
            || c == '{'
            || c == '}'
            || c == '='
            || c == '<'
            || c == '>'
            || c == '!'
            || c == '&'
            || c == '|'
            || c == '"'
            || c == '\''
    }) {
        let word = word.trim_matches(|c| c == '`');
        if word.len() >= 3
            && !common_words.contains(&word)
            && word.contains(|c: char| c == '_' || c.is_uppercase() || c == '.')
        {
            candidates.push(word.to_string());
        }
    }

    candidates.sort();
    candidates.dedup();
    candidates
}

fn truncate(s: &str, max_len: usize) -> &str {
    if s.len() <= max_len {
        s
    } else {
        // Find the nearest valid char boundary at or before max_len to avoid
        // panicking on multi-byte UTF-8 characters.
        match s.char_indices().take_while(|(i, _)| *i <= max_len).last() {
            Some((i, c)) => &s[..i + c.len_utf8()],
            None => "",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use snif_types::{FileLocation, FindingCategory};

    fn make_finding(file: &str, evidence: &str, confidence: f64) -> Finding {
        Finding {
            location: FileLocation {
                file: file.to_string(),
                start_line: 10,
                end_line: None,
            },
            category: FindingCategory::Logic,
            confidence,
            evidence: evidence.to_string(),
            explanation: "Test explanation".to_string(),
            impact: "Test impact".to_string(),
            suggestion: None,
            fingerprint: None,
        }
    }

    #[test]
    fn evidence_found_in_file_passes_verification() {
        let dir = std::env::temp_dir();
        let file_path = dir.join("test_verify.rs");
        std::fs::write(&file_path, "fn delete_summaries_for_files() {}\n").unwrap();

        let finding = make_finding(
            file_path.to_str().unwrap(),
            "Calls `delete_summaries_for_files`",
            0.95,
        );
        let result = verify_finding(finding, &dir);
        assert_eq!(result.confidence, 0.95);
        let _ = std::fs::remove_file(&file_path);
    }

    #[test]
    fn evidence_not_found_reduces_confidence() {
        let dir = std::env::temp_dir();
        let file_path = dir.join("test_verify2.rs");
        std::fs::write(&file_path, "fn some_other_function() {}\n").unwrap();

        let finding = make_finding(
            file_path.to_str().unwrap(),
            "Calls `get_embedded_chunk_id`",
            0.95,
        );
        let result = verify_finding(finding, &dir);
        assert!(
            result.confidence < 0.95,
            "Confidence should be reduced: got {}",
            result.confidence
        );
        assert!((result.confidence - (0.95 - EVIDENCE_MISMATCH_PENALTY)).abs() < 0.001);
        let _ = std::fs::remove_file(&file_path);
    }

    #[test]
    fn file_not_found_reduces_confidence_less() {
        let dir = std::env::temp_dir();

        let finding = make_finding(
            "nonexistent_file.rs",
            "Calls `delete_summaries_for_files`",
            0.95,
        );
        let result = verify_finding(finding, &dir);
        assert!((result.confidence - (0.95 - FILE_NOT_FOUND_PENALTY)).abs() < 0.001);
    }

    #[test]
    fn no_extractable_evidence_passes() {
        let dir = std::env::temp_dir();
        let file_path = dir.join("test_verify3.rs");
        std::fs::write(&file_path, "fn main() {}\n").unwrap();

        let finding = make_finding(
            file_path.to_str().unwrap(),
            "This code has a logic error",
            0.9,
        );
        let result = verify_finding(finding, &dir);
        // No code references to verify — assume correct
        assert_eq!(result.confidence, 0.9);
        let _ = std::fs::remove_file(&file_path);
    }

    #[test]
    fn backtick_code_references_are_extracted() {
        let candidates = extract_code_candidates("The method `get_embedded_chunk_ids` is called");
        assert!(candidates.contains(&"get_embedded_chunk_ids".to_string()));
    }

    #[test]
    fn identifier_extraction_catches_snake_case() {
        let candidates = extract_code_candidates("delete_summaries_for_files is called");
        assert!(candidates.contains(&"delete_summaries_for_files".to_string()));
    }

    #[test]
    fn penalty_does_not_go_negative() {
        let dir = std::env::temp_dir();
        let file_path = dir.join("test_verify4.rs");
        std::fs::write(&file_path, "fn main() {}\n").unwrap();

        let finding = make_finding(
            file_path.to_str().unwrap(),
            "Calls `nonexistent_method`",
            0.1,
        );
        let result = verify_finding(finding, &dir);
        assert!(result.confidence >= 0.0);
        let _ = std::fs::remove_file(&file_path);
    }

    #[test]
    fn mixed_evidence_partial_match_succeeds() {
        let dir = std::env::temp_dir();
        let file_path = dir.join("test_verify5.rs");
        std::fs::write(&file_path, "fn real_function() {}\n").unwrap();

        // One real reference and one fabricated one — should still verify
        let finding = make_finding(
            file_path.to_str().unwrap(),
            "Calls `real_function` and `fake_function`",
            0.9,
        );
        let result = verify_finding(finding, &dir);
        assert_eq!(result.confidence, 0.9);
        let _ = std::fs::remove_file(&file_path);
    }

    #[test]
    fn truncate_handles_multibyte_utf8() {
        // Japanese characters are 3 bytes each — slicing at byte 80 could land
        // inside a character if we used &s[..80] directly
        let evidence = "This code has a résumé function and some 日本語 characters that exceed eighty bytes total length";
        let truncated = truncate(evidence, 80);
        assert!(
            truncated.len() <= 83,
            "truncated length should be at or near boundary: got {}",
            truncated.len()
        );
        // Verify it's valid UTF-8 (would panic if we sliced mid-character)
        let _ = truncated.to_string();
    }

    #[test]
    fn truncate_short_string_unchanged() {
        let short = "hello";
        assert_eq!(truncate(short, 80), "hello");
    }

    #[test]
    fn truncate_empty_string() {
        assert_eq!(truncate("", 80), "");
    }
}
