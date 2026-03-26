pub const SYSTEM_PROMPT: &str = "\
You are a code documentation expert. You describe what code does and its role \
in the system. You focus on purpose and dependencies, not implementation details. \
You respond with only the summary, no preamble or formatting.";

pub fn function_prompt(file_path: &str, symbol_name: &str, kind: &str, body: &str) -> String {
    format!(
        "Summarize this {} in 2-3 sentences. Describe what it does and its role in the system.\n\n\
         File: {}\n\
         Name: {}\n\n\
         ```\n{}\n```",
        kind, file_path, symbol_name, body
    )
}

pub fn file_prompt(file_path: &str, child_summaries: &[(String, String)]) -> String {
    let mut prompt = format!(
        "Summarize this file in 2-3 sentences. Describe the module's purpose and what it provides.\n\n\
         File: {}\n\n\
         Contents:\n",
        file_path
    );

    for (name, summary) in child_summaries {
        prompt.push_str(&format!("- {}: {}\n", name, summary));
    }

    prompt
}
