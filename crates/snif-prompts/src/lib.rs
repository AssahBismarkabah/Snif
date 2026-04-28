use snif_config::constants::prompts;
use snif_config::SnifConfig;
use snif_types::{ContentTier, ContextPackage};

/// Placeholder token used in prompt template strings.
const PLACEHOLDER: &str = "{}";

/// Replaces the `{}` placeholder in a template string with a value.
fn fill_template(template: &str, value: &str) -> String {
    template.replace(PLACEHOLDER, value)
}

pub fn render_system_prompt(config: &SnifConfig) -> String {
    render_system_prompt_with_conventions(config, None, None)
}

pub fn render_system_prompt_with_conventions(
    config: &SnifConfig,
    conventions: Option<&str>,
    guidance: Option<&str>,
) -> String {
    let mut prompt = String::from(prompts::SYSTEM_PROMPT_INTRO);
    prompt.push_str(prompts::SECTION_SEPARATOR);
    prompt.push_str(prompts::SYSTEM_PROMPT_JSON_INSTRUCTION);
    prompt.push_str(prompts::RULES_HEADER);

    for rule in prompts::SYSTEM_PROMPT_RULES {
        prompt.push_str(rule);
        prompt.push_str(prompts::NEWLINE);
    }

    if config.filter.suppress_style_only {
        prompt.push_str(prompts::SYSTEM_PROMPT_STYLE_SUPPRESSION);
        prompt.push_str(prompts::NEWLINE);
    }

    if let Some(conventions) = conventions {
        prompt.push_str(prompts::NEWLINE);
        prompt.push_str(prompts::SYSTEM_PROMPT_CONVENTION_INSTRUCTION);
        prompt.push_str(conventions);
        prompt.push_str(prompts::SYSTEM_PROMPT_CONVENTION_FOOTER);
    }

    if let Some(g) = guidance {
        prompt.push_str(prompts::NEWLINE);
        prompt.push_str(g);
        prompt.push_str(prompts::NEWLINE);
    }

    prompt.push_str(prompts::SYSTEM_PROMPT_RESPONSE_FORMAT);

    prompt
}

pub fn render_user_prompt(context: &ContextPackage) -> String {
    let mut prompt = String::new();

    if let Some(title) = &context.metadata.title {
        prompt.push_str(&fill_template(prompts::METADATA_CHANGE_LABEL, title));
    }
    if let Some(author) = &context.metadata.author {
        prompt.push_str(&fill_template(prompts::METADATA_AUTHOR_LABEL, author));
    }
    if let Some(branch) = &context.metadata.base_branch {
        prompt.push_str(&fill_template(prompts::METADATA_BRANCH_LABEL, branch));
    }
    if let Some(description) = &context.metadata.description {
        prompt.push_str(&fill_template(
            prompts::METADATA_DESCRIPTION_HEADER,
            description,
        ));
    }
    if !context.metadata.labels.is_empty() {
        prompt.push_str(&fill_template(
            prompts::METADATA_LABELS_HEADER,
            &context.metadata.labels.join(", "),
        ));
    }
    if !context.metadata.commit_messages.is_empty() {
        prompt.push_str(prompts::METADATA_COMMITS_HEADER);
        for msg in &context.metadata.commit_messages {
            prompt.push_str(&fill_template(prompts::METADATA_COMMIT_ITEM, msg));
        }
    }
    prompt.push_str(prompts::NEWLINE);

    prompt.push_str(prompts::USER_PROMPT_DIFF_HEADER);
    prompt.push_str(&context.diff);
    prompt.push_str(prompts::USER_PROMPT_DIFF_FOOTER);

    prompt.push_str(prompts::USER_PROMPT_CHANGED_FILES_HEADER);
    for file in &context.changed_files {
        prompt.push_str(&fill_template(prompts::METADATA_FILE_HEADER, &file.path));
        if let Some(summary) = &file.summary {
            prompt.push_str(&fill_template(prompts::METADATA_SUMMARY_LABEL, summary));
        }
        match file.content_tier {
            ContentTier::Full => {
                prompt.push_str(prompts::CODE_FENCE_OPEN);
                for (i, line) in file.content.lines().enumerate() {
                    prompt.push_str(&fill_template(
                        prompts::USER_PROMPT_LINE_FORMAT,
                        &format!("{:>4} | {}", i + 1, line),
                    ));
                }
                prompt.push_str(prompts::CODE_FENCE_CLOSE);
            }
            ContentTier::SummaryOnly | ContentTier::DiffOnly => {
                prompt.push_str(&fill_template(
                    prompts::USER_PROMPT_DIFF_ONLY_CONTENT,
                    &file.content,
                ));
            }
        }
    }

    if !context.related_files.is_empty() {
        prompt.push_str(prompts::USER_PROMPT_RELATED_FILES_HEADER);
        for file in &context.related_files {
            prompt.push_str(&fill_template(prompts::METADATA_FILE_HEADER, &file.path));
            if let Some(score) = file.retrieval_score {
                prompt.push_str(&fill_template(
                    prompts::USER_PROMPT_RELEVANCE_FORMAT,
                    &format!("{:.2}", score),
                ));
            }
            prompt.push_str(prompts::NEWLINE);
            if let Some(summary) = &file.summary {
                prompt.push_str(&fill_template(prompts::METADATA_SUMMARY_LABEL, summary));
            }
            prompt.push_str(&fill_template(prompts::CODE_FENCE_WRAPPER, &file.content));
        }
    }

    prompt.push_str(prompts::USER_PROMPT_FINAL_INSTRUCTION);
    prompt
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fill_template_replaces_placeholder() {
        assert_eq!(fill_template("Hello, {}!", "World"), "Hello, World!");
    }

    #[test]
    fn fill_template_no_placeholder() {
        assert_eq!(fill_template("No placeholder", "X"), "No placeholder");
    }

    #[test]
    fn render_system_prompt_contains_rules() {
        let config = SnifConfig::default();
        let prompt = render_system_prompt(&config);
        assert!(prompt.contains("Rules:"));
        assert!(prompt.contains(prompts::SYSTEM_PROMPT_INTRO));
        assert!(prompt.contains(prompts::SYSTEM_PROMPT_RESPONSE_FORMAT));
    }

    #[test]
    fn system_prompt_includes_conventions_and_guidance() {
        let config = SnifConfig::default();
        let prompt = super::render_system_prompt_with_conventions(
            &config,
            Some("no debug prints"),
            Some("extra guidance"),
        );
        assert!(prompt.contains("Project Conventions"));
        assert!(prompt.contains("no debug prints"));
        assert!(prompt.contains("extra guidance"));
    }

    #[test]
    fn render_system_prompt_style_suppression() {
        let mut config = SnifConfig::default();
        config.filter.suppress_style_only = true;
        let prompt = render_system_prompt(&config);
        assert!(prompt.contains("style-only"));
    }

    #[test]
    fn render_system_prompt_no_style_suppression() {
        let mut config = SnifConfig::default();
        config.filter.suppress_style_only = false;
        let prompt = render_system_prompt(&config);
        assert!(!prompt.contains("style-only"));
    }
}
