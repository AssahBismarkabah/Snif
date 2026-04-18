use snif_config::constants::prompts;
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
    let mut prompt = String::from(prompts::SYSTEM_PROMPT_INTRO);
    prompt.push_str("\n\n");
    prompt.push_str(prompts::SYSTEM_PROMPT_JSON_INSTRUCTION);
    prompt.push_str("\n\nRules:\n");

    for rule in prompts::SYSTEM_PROMPT_RULES {
        prompt.push_str(rule);
        prompt.push('\n');
    }

    if config.filter.suppress_style_only {
        prompt.push_str(prompts::SYSTEM_PROMPT_STYLE_SUPPRESSION);
        prompt.push('\n');
    }

    if let Some(conventions) = conventions {
        prompt.push('\n');
        prompt.push_str(prompts::SYSTEM_PROMPT_CONVENTION_INSTRUCTION);
        prompt.push_str(conventions);
        prompt.push_str(prompts::SYSTEM_PROMPT_CONVENTION_FOOTER);
    }

    if let Some(g) = guidance {
        prompt.push('\n');
        prompt.push_str(g);
        prompt.push('\n');
    }

    prompt.push_str(prompts::SYSTEM_PROMPT_RESPONSE_FORMAT);

    prompt
}

pub fn render_user_prompt(context: &ContextPackage) -> String {
    let mut prompt = String::new();

    if let Some(title) = &context.metadata.title {
        prompt.push_str(&prompts::METADATA_CHANGE_LABEL.replace("{}", title));
    }
    if let Some(author) = &context.metadata.author {
        prompt.push_str(&prompts::METADATA_AUTHOR_LABEL.replace("{}", author));
    }
    if let Some(branch) = &context.metadata.base_branch {
        prompt.push_str(&prompts::METADATA_BRANCH_LABEL.replace("{}", branch));
    }
    if let Some(description) = &context.metadata.description {
        prompt.push_str(&prompts::METADATA_DESCRIPTION_HEADER.replace("{}", description));
    }
    if !context.metadata.labels.is_empty() {
        prompt.push_str(
            &prompts::METADATA_LABELS_HEADER.replace("{}", &context.metadata.labels.join(", ")),
        );
    }
    if !context.metadata.commit_messages.is_empty() {
        prompt.push_str(prompts::METADATA_COMMITS_HEADER);
        for msg in &context.metadata.commit_messages {
            prompt.push_str(&prompts::METADATA_COMMIT_ITEM.replace("{}", msg));
        }
    }
    prompt.push('\n');

    prompt.push_str(prompts::USER_PROMPT_DIFF_HEADER);
    prompt.push_str(&context.diff);
    prompt.push_str(prompts::USER_PROMPT_DIFF_FOOTER);

    prompt.push_str(prompts::USER_PROMPT_CHANGED_FILES_HEADER);
    for file in &context.changed_files {
        prompt.push_str(&prompts::METADATA_FILE_HEADER.replace("{}", &file.path));
        if let Some(summary) = &file.summary {
            prompt.push_str(&prompts::METADATA_SUMMARY_LABEL.replace("{}", summary));
        }
        match file.content_tier {
            ContentTier::Full => {
                prompt.push_str("```\n");
                for (i, line) in file.content.lines().enumerate() {
                    prompt.push_str(
                        &prompts::USER_PROMPT_LINE_FORMAT
                            .replace("{}", &format!("{:>4} | {}", i + 1, line)),
                    );
                }
                prompt.push_str("```\n\n");
            }
            ContentTier::SummaryOnly | ContentTier::DiffOnly => {
                prompt
                    .push_str(&prompts::USER_PROMPT_DIFF_ONLY_CONTENT.replace("{}", &file.content));
            }
        }
    }

    if !context.related_files.is_empty() {
        prompt.push_str(prompts::USER_PROMPT_RELATED_FILES_HEADER);
        for file in &context.related_files {
            prompt.push_str(&prompts::METADATA_FILE_HEADER.replace("{}", &file.path));
            if let Some(score) = file.retrieval_score {
                prompt.push_str(
                    &prompts::USER_PROMPT_RELEVANCE_FORMAT.replace("{}", &format!("{:.2}", score)),
                );
            }
            prompt.push('\n');
            if let Some(summary) = &file.summary {
                prompt.push_str(&prompts::METADATA_SUMMARY_LABEL.replace("{}", summary));
            }
            prompt.push_str(&format!("```\n{}\n```\n\n", file.content));
        }
    }

    prompt.push_str(prompts::USER_PROMPT_FINAL_INSTRUCTION);
    prompt
}
