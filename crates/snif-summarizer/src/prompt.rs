pub const SYSTEM_PROMPT: &str = "\
You are a code documentation expert. You describe what code does and its role \
in the system. You focus on purpose and dependencies, not implementation details. \
You respond with only the summary, no preamble or formatting.";

/// System prompt for batch summarization. Asks the LLM to return summaries as a
/// JSON object keyed by symbol name, which the batch parser then extracts.
pub const BATCH_SYSTEM_PROMPT: &str = "\
You are a code documentation expert. You describe what code does and its role \
in the system. You focus on purpose and dependencies, not implementation details. \
When given multiple functions, respond with a single JSON object where each key is \
the function name and each value is a 2-3 sentence summary. Output only valid JSON, \
no markdown fences, no preamble, no trailing text. Example: \
{\"fn_name\": \"Summary of what fn_name does and its role.\"}";

pub fn function_prompt(file_path: &str, symbol_name: &str, kind: &str, body: &str) -> String {
    format!(
        "Summarize this {} in 2-3 sentences. Describe what it does and its role in the system.\n\n\
         File: {}\n\
         Name: {}\n\n\
         ```\n{}\n```",
        kind, file_path, symbol_name, body
    )
}

/// Generate a batch prompt for multiple symbols from the same file.
/// Asks the LLM to return a JSON array of summaries keyed by symbol name.
pub fn batch_prompt(file_path: &str, symbols: &[(String, String, String)]) -> String {
    let mut prompt = format!(
        "Summarize each function below in 2-3 sentences. Describe what it does and \
         its role in the system. File: {}\n\n\
         Respond with a JSON object where each key is the function name and each \
         value is the summary text. Example format:\n\
         {{\"functionName\": \"Summary of what this function does and its role.\"}}\n\n",
        file_path
    );

    for (name, kind, body) in symbols {
        prompt.push_str(&format!("### {} ({})\n```\n{}\n```\n\n", name, kind, body));
    }

    prompt
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
