use std::fs;
use std::path::Path;

pub fn save_config(path: &Path, content: &str) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let _ = fs::write(path, content);
    Ok(())
}

pub fn load_config(path: &Path) -> Result<String, String> {
    fs::read_to_string(path).map_err(|e| format!("Failed to load config: {}", e))
}
