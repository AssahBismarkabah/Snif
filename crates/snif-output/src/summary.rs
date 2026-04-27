use snif_config::constants::summary;
use snif_types::{Finding, RetrievalResults};

pub struct ReviewSummaryInput<'a> {
    pub change_summary: &'a str,
    pub findings: &'a [Finding],
    pub changed_paths: &'a [String],
    pub retrieval_results: &'a RetrievalResults,
    pub diff_lines: usize,
    pub model_name: &'a str,
    pub duration_secs: u64,
}

pub fn format_pr_summary(input: &ReviewSummaryInput) -> String {
    let mut summary_text = String::from(summary::PR_SUMMARY_HEADER);

    // Walkthrough
    if !input.change_summary.is_empty() {
        summary_text.push_str(&format!("{}\n\n", input.change_summary));
    }

    // Outcome
    if input.findings.is_empty() {
        summary_text.push_str(summary::NO_FINDINGS);
    } else {
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

    // Collapsible details
    summary_text.push_str(summary::COLLAPSIBLE_DETAILS_OPENER);

    // Changed files
    summary_text.push_str(summary::CHANGED_FILES_HEADER);
    for path in input.changed_paths {
        summary_text.push_str(&format!("- `{}`\n", path));
    }

    // Context analyzed
    let related_count = input.retrieval_results.results.len();
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
        summary_text.push_str(".\n");
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
