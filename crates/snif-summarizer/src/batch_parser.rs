/// A single summary parsed from a batch response.
#[derive(Debug, Clone)]
pub struct ParsedBatchSummary {
    pub symbol_name: String,
    pub summary: String,
}

/// Parse a batch summarization response. The LLM is asked to return a JSON
/// object mapping symbol names to summary text. This parser handles:
///
/// - Clean JSON: `{"fn_name": "summary", "other_fn": "summary"}`
/// - JSON wrapped in markdown code fences
/// - JSON with trailing text after the closing brace
/// - Partial failures: returns whatever summaries were successfully parsed
pub fn parse_batch_response(response: &str) -> Vec<ParsedBatchSummary> {
    let json_str = match extract_json_object(response) {
        Some(s) => s,
        None => return Vec::new(),
    };

    match serde_json::from_str::<serde_json::Value>(json_str) {
        Ok(serde_json::Value::Object(map)) => {
            let mut results = Vec::new();
            for (key, value) in map {
                if let Some(text) = value.as_str() {
                    let summary: String = text.trim().to_string();
                    if !summary.is_empty() {
                        results.push(ParsedBatchSummary {
                            symbol_name: key.clone(),
                            summary,
                        });
                    }
                }
            }
            results
        }
        _ => Vec::new(),
    }
}

/// Extract the outermost balanced JSON object from a response that may contain
/// chain-of-thought preamble text or markdown code fences.
fn extract_json_object(input: &str) -> Option<&str> {
    let start = input.find('{')?;
    let mut depth = 0i32;
    let mut in_string = false;
    let mut escape = false;

    for (i, ch) in input[start..].char_indices() {
        update_json_state(ch, &mut in_string, &mut escape);

        if !in_string {
            if ch == '{' {
                depth += 1;
            } else if ch == '}' {
                depth -= 1;
                if depth == 0 {
                    return Some(&input[start..start + i + 1]);
                }
            }
        }
    }

    None
}

fn update_json_state(ch: char, in_string: &mut bool, escape: &mut bool) {
    if *in_string {
        if *escape {
            *escape = false;
        } else if ch == '\\' {
            *escape = true;
        } else if ch == '"' {
            *in_string = false;
        }
    } else if ch == '"' {
        *in_string = true;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_clean_json_batch_response() {
        let response = r#"{"parse_line": "Parses a single line of input into tokens.", "format_date": "Formats a date object into a string."}"#;

        let results = parse_batch_response(response);
        assert_eq!(results.len(), 2);
        let names: Vec<&str> = results.iter().map(|r| r.symbol_name.as_str()).collect();
        assert!(names.contains(&"parse_line"));
        assert!(names.contains(&"format_date"));
        let parse_line = results
            .iter()
            .find(|r| r.symbol_name == "parse_line")
            .unwrap();
        assert!(parse_line.summary.contains("Parses"));
    }

    #[test]
    fn parse_json_with_preamble() {
        let response = "Here are the summaries:\n\n{\"validate\": \"Checks input validity.\", \"sanitize\": \"Cleans user input.\"}";

        let results = parse_batch_response(response);
        assert_eq!(results.len(), 2);
        let names: Vec<&str> = results.iter().map(|r| r.symbol_name.as_str()).collect();
        assert!(names.contains(&"validate"));
        assert!(names.contains(&"sanitize"));
    }

    #[test]
    fn parse_json_with_trailing_text() {
        let response =
            "{\"handle_request\": \"Processes incoming HTTP requests.\"} And that's all.";

        let results = parse_batch_response(response);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].symbol_name, "handle_request");
    }

    #[test]
    fn parse_markdown_wrapped_json() {
        let response = "```json\n{\"compute\": \"Calculates values from input data.\"}\n```";

        let results = parse_batch_response(response);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].symbol_name, "compute");
    }

    #[test]
    fn parse_empty_or_invalid_response() {
        let results = parse_batch_response("I couldn't generate summaries.");
        assert!(results.is_empty());
    }

    #[test]
    fn parse_partial_batch_some_values_not_strings() {
        let response = r#"{"good_fn": "A valid summary.", "bad_fn": null}"#;

        let results = parse_batch_response(response);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].symbol_name, "good_fn");
    }
}
