use snif_config::SnifConfig;
use snif_types::ContextPackage;

pub fn render_system_prompt(config: &SnifConfig) -> String {
    let mut prompt = String::from(
        "You are a strict, precision-focused code reviewer. Your job is to find real issues \
         in code changes — bugs, security vulnerabilities, logic errors, and convention \
         violations that have concrete impact.\n\n\
         Rules:\n\
         - Bias toward false negatives over false positives. If you are not confident, stay quiet.\n\
         - Every finding MUST cite specific evidence from the provided code.\n\
         - Every finding MUST explain the user-relevant impact — what breaks, what is at risk.\n\
         - Do NOT flag speculative or hypothetical issues.\n\
         - Do NOT flag issues you cannot ground in the provided context.\n",
    );

    if config.filter.suppress_style_only {
        prompt.push_str(
            "- Do NOT flag style-only issues (formatting, naming preferences) \
             unless they violate an explicit project convention.\n",
        );
    }

    prompt.push_str(
        "\nRespond with a JSON array of findings. If the change is clean \
         and you find no real issues, respond with an empty array: []\n\n\
         Line numbers MUST refer to the line numbers in the file content \
         provided in the Changed Files section, NOT the diff hunk headers.\n\n\
         Each finding must follow this schema:\n\
         {\n\
           \"file\": \"path/to/file\",\n\
           \"start_line\": <line number in the file>,\n\
           \"end_line\": <line number in the file or null>,\n\
           \"category\": \"logic\" | \"security\" | \"convention\" | \"performance\" | \"style\" | \"other\",\n\
           \"confidence\": <0.0 to 1.0>,\n\
           \"evidence\": \"<quoted code from the diff or context>\",\n\
           \"explanation\": \"<what is wrong and why>\",\n\
           \"impact\": \"<what happens if this is not fixed>\",\n\
           \"suggestion\": \"<optional fix suggestion or null>\"\n\
         }\n",
    );

    prompt
}

pub fn render_user_prompt(context: &ContextPackage) -> String {
    let mut prompt = String::new();

    if let Some(title) = &context.metadata.title {
        prompt.push_str(&format!("Change: {}\n", title));
    }
    if let Some(author) = &context.metadata.author {
        prompt.push_str(&format!("Author: {}\n", author));
    }
    if let Some(branch) = &context.metadata.base_branch {
        prompt.push_str(&format!("Base branch: {}\n", branch));
    }
    prompt.push('\n');

    prompt.push_str("## Diff\n\n```diff\n");
    prompt.push_str(&context.diff);
    prompt.push_str("\n```\n\n");

    prompt.push_str("## Changed Files\n\n");
    for file in &context.changed_files {
        prompt.push_str(&format!("### {}\n", file.path));
        if let Some(summary) = &file.summary {
            prompt.push_str(&format!("Summary: {}\n", summary));
        }
        prompt.push_str("```\n");
        for (i, line) in file.content.lines().enumerate() {
            prompt.push_str(&format!("{:>4} | {}\n", i + 1, line));
        }
        prompt.push_str("```\n\n");
    }

    if !context.related_files.is_empty() {
        prompt.push_str("## Related Files (for context)\n\n");
        for file in &context.related_files {
            prompt.push_str(&format!("### {}", file.path));
            if let Some(score) = file.retrieval_score {
                prompt.push_str(&format!(" (relevance: {:.2})", score));
            }
            prompt.push('\n');
            if let Some(summary) = &file.summary {
                prompt.push_str(&format!("Summary: {}\n", summary));
            }
            prompt.push_str(&format!("```\n{}\n```\n\n", file.content));
        }
    }

    prompt.push_str("Review the diff above. Return your findings as a JSON array.\n");
    prompt
}
