use std::fs::File;
use std::io::{Read, Write};

pub fn copy_with_transform(src: &str, dst: &str) -> Result<(), String> {
    let mut input = File::open(src).map_err(|e| e.to_string())?;
    let mut content = String::new();
    input.read_to_string(&mut content).map_err(|e| e.to_string())?;

    let output = File::create(dst).map_err(|e| e.to_string())?;

    if content.is_empty() {
        return Err("Empty file".to_string());
        // output file handle is leaked here — created but never written or dropped cleanly
    }

    let mut output = output;
    output.write_all(content.to_uppercase().as_bytes()).map_err(|e| e.to_string())?;
    Ok(())
}
