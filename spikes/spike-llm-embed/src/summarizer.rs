use anyhow::{bail, Result};
use std::process::Command;
use std::time::Instant;

pub struct SummaryResult {
    pub summary: String,
    pub duration: std::time::Duration,
    pub input_chars: usize,
    pub output_chars: usize,
}

pub fn summarize_code_unit(
    code: &str,
    file_path: &str,
    symbol_name: &str,
    model: &str,
) -> Result<SummaryResult> {
    let prompt = format!(
        "You are a code documentation expert. Summarize the following code unit in 2-3 sentences. \
         Describe WHAT it does and its role in the system. Do not describe HOW it is implemented. \
         Focus on purpose, dependencies, and what depends on it.\n\n\
         File: {}\n\
         Symbol: {}\n\n\
         ```\n{}\n```\n\n\
         Respond with only the summary, no preamble or formatting.",
        file_path, symbol_name, code
    );

    let input_chars = prompt.len();
    let start = Instant::now();

    let output = Command::new("opencode")
        .arg("run")
        .arg("--format")
        .arg("json")
        .arg("--model")
        .arg(model)
        .arg(&prompt)
        .output()?;

    let duration = start.elapsed();

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("OpenCode failed (exit {}): {}", output.status, stderr);
    }

    let stdout = String::from_utf8(output.stdout)?;

    // Parse JSON events from opencode output — look for text content
    let summary = extract_text_from_opencode_json(&stdout)?;
    let output_chars = summary.len();

    Ok(SummaryResult {
        summary,
        duration,
        input_chars,
        output_chars,
    })
}

fn extract_text_from_opencode_json(output: &str) -> Result<String> {
    let mut text_parts = Vec::new();

    for line in output.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        if let Ok(value) = serde_json::from_str::<serde_json::Value>(line) {
            // opencode JSON format emits events — look for text content
            if let Some(text) = value.get("text").and_then(|t| t.as_str()) {
                text_parts.push(text.to_string());
            }
            // Also check for content in message format
            if let Some(content) = value.get("content").and_then(|c| c.as_str()) {
                text_parts.push(content.to_string());
            }
            // Check for assistant message parts
            if let Some(parts) = value.get("parts").and_then(|p| p.as_array()) {
                for part in parts {
                    if let Some(text) = part.get("text").and_then(|t| t.as_str()) {
                        text_parts.push(text.to_string());
                    }
                    if let Some(text) = part.as_str() {
                        text_parts.push(text.to_string());
                    }
                }
            }
        }
    }

    if text_parts.is_empty() {
        // Fall back to raw output if JSON parsing doesn't find structured text
        let cleaned = output.trim().to_string();
        if cleaned.is_empty() {
            bail!("OpenCode returned empty output");
        }
        return Ok(cleaned);
    }

    Ok(text_parts.join("").trim().to_string())
}
