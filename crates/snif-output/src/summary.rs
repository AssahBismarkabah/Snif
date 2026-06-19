use snif_config::constants::summary;
use snif_types::{Finding, RetrievalResults};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReviewOutcome {
    Findings,
    Clean,
    LimitedClean,
    Inconclusive,
}

pub struct ReviewSummaryInput<'a> {
    pub change_summary: &'a str,
    pub findings: &'a [Finding],
    pub outcome: ReviewOutcome,
    pub changed_paths: &'a [String],
    pub retrieval_results: &'a RetrievalResults,
    pub related_files_analyzed: usize,
    pub diff_lines: usize,
    pub model_name: &'a str,
    pub duration_secs: u64,
    pub context_note: Option<&'a str>,
}

pub fn format_pr_summary(input: &ReviewSummaryInput) -> String {
    let mut summary_text = String::from(summary::PR_SUMMARY_HEADER);

    // Walkthrough
    if input.outcome != ReviewOutcome::Inconclusive && !input.change_summary.is_empty() {
        summary_text.push_str(&format!("{}\n\n", input.change_summary));
    }

    // Outcome
    match input.outcome {
        ReviewOutcome::Inconclusive => summary_text.push_str(summary::INCONCLUSIVE_REVIEW),
        ReviewOutcome::LimitedClean if input.findings.is_empty() => {
            summary_text.push_str(summary::LIMITED_NO_FINDINGS);
        }
        ReviewOutcome::Clean if input.findings.is_empty() => {
            summary_text.push_str(summary::NO_FINDINGS);
        }
        ReviewOutcome::Findings if !input.findings.is_empty() => {
            let suffix = if input.findings.len() == 1 {
                summary::FINDINGS_FOUND_SUFFIX_SINGULAR
            } else {
                summary::FINDINGS_FOUND_SUFFIX_PLURAL
            };
            summary_text.push_str(&format!(
                "{}{}{}",
                summary::FINDINGS_FOUND_PREFIX,
                input.findings.len(),
                suffix
            ));
        }
        _ => summary_text.push_str(summary::NO_FINDINGS),
    }

    // Collapsible details
    summary_text.push_str(summary::COLLAPSIBLE_DETAILS_OPENER);

    // Changed files
    summary_text.push_str(summary::CHANGED_FILES_HEADER);
    for path in input.changed_paths {
        summary_text.push_str(&format!("- `{}`\n", path));
    }

    // Context analyzed
    let related_count = input.related_files_analyzed;
    if related_count > 0 {
        let file_suffix = if related_count == 1 {
            summary::CONTEXT_ANALYZED_FILE_SUFFIX_SINGULAR
        } else {
            summary::CONTEXT_ANALYZED_FILES_SUFFIX_PLURAL
        };
        summary_text.push_str(&format!(
            "{}{}{}",
            summary::CONTEXT_ANALYZED_HEADER,
            related_count,
            file_suffix
        ));

        if related_count == input.retrieval_results.results.len() {
            let mut methods = Vec::new();
            if input.retrieval_results.structural_count > 0 {
                methods.push(format!(
                    "{}{}",
                    input.retrieval_results.structural_count,
                    summary::STRUCTURAL_RETRIEVAL_LABEL
                ));
            }
            if input.retrieval_results.semantic_count > 0 {
                methods.push(format!(
                    "{}{}",
                    input.retrieval_results.semantic_count,
                    summary::SEMANTIC_RETRIEVAL_LABEL
                ));
            }
            if input.retrieval_results.code_semantic_count > 0 {
                methods.push(format!(
                    "{}{}",
                    input.retrieval_results.code_semantic_count,
                    summary::CODE_SEMANTIC_RETRIEVAL_LABEL
                ));
            }
            if input.retrieval_results.keyword_count > 0 {
                methods.push(format!(
                    "{}{}",
                    input.retrieval_results.keyword_count,
                    summary::KEYWORD_RETRIEVAL_LABEL
                ));
            }
            if !methods.is_empty() {
                summary_text.push_str(&format!(" ({})", methods.join(summary::METHODS_SEPARATOR)));
            }
        } else {
            summary_text.push_str(summary::CONTEXT_ANALYZED_TRIMMED_SUFFIX);
        }
        summary_text.push_str(".\n");
    }

    if let Some(note) = input.context_note {
        summary_text.push_str("\n**Context note:** ");
        summary_text.push_str(note);
        summary_text.push('\n');
    }

    // Stats
    summary_text.push_str(summary::STATS_LINE_PREFIX);
    summary_text.push_str(&input.diff_lines.to_string());
    summary_text.push_str(summary::STATS_LINE_SUFFIX);
    summary_text.push_str(input.model_name);
    summary_text.push_str(summary::STATS_LINE_MODEL_SUFFIX);
    summary_text.push_str(&input.duration_secs.to_string());
    summary_text.push_str(summary::STATS_LINE_SECONDS_SUFFIX);

    summary_text.push_str(summary::COLLAPSIBLE_DETAILS_CLOSER);

    summary_text
}

#[cfg(test)]
mod tests {
    use super::*;
    use snif_types::RetrievalResults;

    fn retrieval_results(count: usize) -> RetrievalResults {
        RetrievalResults {
            results: (0..count)
                .map(|i| snif_types::RetrievalResult {
                    file_id: i as i64,
                    path: format!("src/{i}.rs"),
                    score: 1.0,
                    sources: vec![],
                })
                .collect(),
            structural_count: 0,
            semantic_count: 0,
            code_semantic_count: 0,
            keyword_count: count,
        }
    }

    #[test]
    fn clean_summary_uses_no_issues_wording() {
        let retrieval = retrieval_results(0);
        let summary = format_pr_summary(&ReviewSummaryInput {
            change_summary: "Clean refactor.",
            findings: &[],
            outcome: ReviewOutcome::Clean,
            changed_paths: &["src/lib.rs".to_string()],
            retrieval_results: &retrieval,
            related_files_analyzed: 0,
            diff_lines: 10,
            model_name: "test-model",
            duration_secs: 1,
            context_note: None,
        });

        assert!(summary.contains("**No issues found.** Change looks clean."));
    }

    #[test]
    fn limited_clean_summary_uses_limited_wording() {
        let retrieval = retrieval_results(2);
        let summary = format_pr_summary(&ReviewSummaryInput {
            change_summary: "Large refactor.",
            findings: &[],
            outcome: ReviewOutcome::LimitedClean,
            changed_paths: &["src/lib.rs".to_string()],
            retrieval_results: &retrieval,
            related_files_analyzed: 1,
            diff_lines: 10,
            model_name: "test-model",
            duration_secs: 1,
            context_note: Some("Context was reduced."),
        });

        assert!(summary.contains("No reportable issues found in analyzed context"));
        assert!(summary.contains("**Context analyzed:** 1 related file included after trimming."));
    }

    #[test]
    fn inconclusive_summary_never_says_clean_or_repeats_untrusted_summary() {
        let retrieval = retrieval_results(0);
        let summary = format_pr_summary(&ReviewSummaryInput {
            change_summary: "Two security concerns identified.",
            findings: &[],
            outcome: ReviewOutcome::Inconclusive,
            changed_paths: &["src/lib.rs".to_string()],
            retrieval_results: &retrieval,
            related_files_analyzed: 0,
            diff_lines: 10,
            model_name: "test-model",
            duration_secs: 1,
            context_note: Some("Review was inconclusive."),
        });

        assert!(summary.contains("Review inconclusive"));
        assert!(!summary.contains("No issues found"));
        assert!(!summary.contains("Two security concerns identified."));
    }
}
