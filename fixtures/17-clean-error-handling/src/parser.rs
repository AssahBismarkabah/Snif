use std::num::ParseIntError;

pub fn parse_port(s: &str) -> Result<u16, ParseIntError> {
    s.trim().parse::<u16>()
}

pub fn parse_host_port(input: &str) -> Result<(String, u16), String> {
    let parts: Vec<&str> = input.splitn(2, ':').collect();
    if parts.len() != 2 {
        return Err(format!("Invalid host:port format: {}", input));
    }
    let host = parts[0].to_string();
    let port = parse_port(parts[1]).map_err(|e| format!("Invalid port: {}", e))?;
    Ok((host, port))
}
