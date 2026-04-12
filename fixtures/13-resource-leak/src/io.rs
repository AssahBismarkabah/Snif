use std::fs::File;
use std::io::{Read, Write};
use std::os::fd::{FromRawFd, IntoRawFd};

pub fn copy_with_transform(src: &str, dst: &str) -> Result<(), String> {
    let mut input = File::open(src).map_err(|e| e.to_string())?;
    let mut content = String::new();
    input.read_to_string(&mut content).map_err(|e| e.to_string())?;

    let raw_fd = File::create(dst).map_err(|e| e.to_string())?.into_raw_fd();

    if content.is_empty() {
        return Err("Empty file".to_string());
        // raw_fd is leaked here because no File is reconstructed to close it
    }

    let mut output = unsafe { File::from_raw_fd(raw_fd) };
    output.write_all(content.to_uppercase().as_bytes()).map_err(|e| e.to_string())?;
    Ok(())
}
