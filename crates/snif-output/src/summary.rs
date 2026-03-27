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
    let mut summary = String::from("## Snif Review\n\n");

    // Walkthrough
    if !input.change_summary.is_empty() {
        summary.push_str(&format!("{}\n\n", input.change_summary));
    }

    // Outcome
    if input.findings.is_empty() {
        summary.push_str(":white_check_mark: **No issues found.** Change looks clean.\n\n");
    } else {
        summary.push_str(&format!(
            ":mag: **Found {} issue{}.** See inline comments for details.\n\n",
            input.findings.len(),
            if input.findings.len() == 1 { "" } else { "s" }
        ));
    }

    // Collapsible details
    summary.push_str("<details>\n<summary>Review details</summary>\n\n");

    // Changed files
    summary.push_str("**Changed files:**\n");
    for path in input.changed_paths {
        summary.push_str(&format!("- `{}`\n", path));
    }

    // Context analyzed
    let related_count = input.retrieval_results.results.len();
    if related_count > 0 {
        summary.push_str(&format!(
            "\n**Context analyzed:** {} related file{}",
            related_count,
            if related_count == 1 { "" } else { "s" }
        ));

        let mut methods = Vec::new();
        if input.retrieval_results.structural_count > 0 {
            methods.push(format!(
                "{} via imports/dependencies",
                input.retrieval_results.structural_count
            ));
        }
        if input.retrieval_results.semantic_count > 0 {
            methods.push(format!(
                "{} via semantic similarity",
                input.retrieval_results.semantic_count
            ));
        }
        if input.retrieval_results.keyword_count > 0 {
            methods.push(format!(
                "{} via symbol matching",
                input.retrieval_results.keyword_count
            ));
        }
        if !methods.is_empty() {
            summary.push_str(&format!(" ({})", methods.join(", ")));
        }
        summary.push_str(".\n");
    }

    // Stats
    summary.push_str(&format!(
        "\n**Stats:** {} lines | `{}` | {}s\n",
        input.diff_lines, input.model_name, input.duration_secs,
    ));

    summary.push_str("\n</details>\n");

    summary
}
