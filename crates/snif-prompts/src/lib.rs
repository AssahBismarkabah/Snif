use snif_config::SnifConfig;
use snif_types::{ContentTier, ContextPackage};

pub fn render_system_prompt(config: &SnifConfig) -> String {
    render_system_prompt_with_conventions(config, None, None)
}

pub fn render_system_prompt_with_conventions(
    config: &SnifConfig,
    conventions: Option<&str>,
    guidance: Option<&str>,
) -> String {
    let mut prompt = String::from(
        "You are a strict, precision-focused code reviewer. Your job is to find real issues \
         in code changes — bugs, security vulnerabilities, logic errors, and convention \
         violations that have concrete impact.\n\n\
         Return ONLY one valid JSON object. Do not output markdown fences, analysis, \
         step-by-step reasoning, or any text before or after the JSON. Your first character \
         must be '{' and your last character must be '}'.\n\n\
         Rules:\n\
         - Bias toward false negatives over false positives. If you are not confident, stay quiet.\n\
         - Keep reasoning internal. Never expose chain-of-thought.\n\
         - Every finding MUST cite specific evidence from the provided code.\n\
         - Every finding MUST explain the user-relevant impact — what breaks, what is at risk.\n\
         - Do NOT flag speculative or hypothetical issues.\n\
         - Do NOT flag issues you cannot ground in the provided context.\n\
         - Prefer one finding per distinct root cause. Do not split one underlying bug into \
         multiple overlapping findings.\n\
         - Do NOT flag micro-optimizations (unnecessary allocations, format patterns, iterator \
         vs collect, clone vs borrow) unless the code is in a measured hot path or processes \
         unbounded input. Focus on bugs that break correctness or security.\n\
         - Treat database queries inside loops as real performance bugs.\n\
         - Treat reading or collecting unbounded user-controlled input into memory without \
         size limits as a real performance bug.\n\
         - Treat joining user-controlled path segments onto a base directory without validation \
         or normalization as a security bug.\n\
         - Treat recursive or generic merges of user-controlled objects into plain objects \
         without blocking \"__proto__\", \"prototype\", or \"constructor\" keys as a \
         security bug (prototype pollution).\n\
         - If full file content is not provided for a changed file, use the diff hunks to review \
         that file's changes.\n",
    );

    if config.filter.suppress_style_only {
        prompt.push_str(
            "- Do NOT flag style-only issues (formatting, naming preferences) \
             unless they violate an explicit project convention.\n",
        );
    }

    if let Some(conventions) = conventions {
        prompt.push_str("\n## Project Conventions\n\n");
        prompt.push_str(conventions);
        prompt.push_str("\n\nFlag violations of these conventions with category \"convention\".\n");
    }

    if let Some(g) = guidance {
        prompt.push('\n');
        prompt.push_str(g);
        prompt.push('\n');
    }

    prompt.push_str(
        "\nRespond with a JSON object containing two fields:\n\n\
         1. \"summary\": A 1-2 sentence walkthrough of what this change does and why. \
         Describe the intent and impact on the codebase, not the individual files.\n\n\
         2. \"findings\": A JSON array of issues found. If the change is clean, \
         use an empty array. If you are unsure about the format, return \
         {\"summary\":\"\",\"findings\":[]} exactly.\n\n\
         Line numbers MUST refer to the line numbers in the file content \
         provided in the Changed Files section. If file content is omitted, \
         use the line numbers from the diff hunks.\n\n\
         Response format:\n\
         {\n\
           \"summary\": \"<1-2 sentence walkthrough of the change>\",\n\
           \"findings\": [\n\
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
             }\n\
           ]\n\
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
    if let Some(description) = &context.metadata.description {
        prompt.push_str(&format!("\nDescription:\n{}\n", description));
    }
    if !context.metadata.labels.is_empty() {
        prompt.push_str(&format!(
            "\nLabels: {}\n",
            context.metadata.labels.join(", ")
        ));
    }
    if !context.metadata.commit_messages.is_empty() {
        prompt.push_str("\nCommits:\n");
        for msg in &context.metadata.commit_messages {
            prompt.push_str(&format!("- {}\n", msg));
        }
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
        match file.content_tier {
            ContentTier::Full => {
                prompt.push_str("```\n");
                for (i, line) in file.content.lines().enumerate() {
                    prompt.push_str(&format!("{:>4} | {}\n", i + 1, line));
                }
                prompt.push_str("```\n\n");
            }
            ContentTier::SummaryOnly | ContentTier::DiffOnly => {
                prompt.push_str(&format!("*{}*\n\n", file.content));
            }
        }
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

    prompt.push_str(
        "Review the diff above. Return only the JSON object described in the system prompt. \
         Do not include markdown fences, analysis, or any extra text. Your first character \
         must be '{' and your last character must be '}'. If you are unsure, return \
         {\"summary\":\"\",\"findings\":[]} exactly.\n",
    );
    prompt
}
