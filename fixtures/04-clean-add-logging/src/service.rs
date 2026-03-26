use tracing::{info, debug};

pub fn handle_request(path: &str, body: &[u8]) -> Result<Vec<u8>, String> {
    info!(path = %path, body_len = body.len(), "Handling request");
    let parsed = parse_body(body).map_err(|e| format!("Parse error: {}", e))?;
    let result = process(parsed)?;
    info!(result_len = result.len(), "Request processed");
    Ok(result)
}

fn parse_body(body: &[u8]) -> Result<String, String> {
    debug!(len = body.len(), "Parsing body");
    String::from_utf8(body.to_vec()).map_err(|e| e.to_string())
}

fn process(input: String) -> Result<Vec<u8>, String> {
    Ok(input.into_bytes())
}
