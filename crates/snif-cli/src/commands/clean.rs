use anyhow::Result;
use std::path::Path;

pub fn run(path: &str) -> Result<()> {
    let repo_path = Path::new(path);

    let targets = [".snif", ".fastembed_cache"];

    let mut cleaned = false;

    for target in &targets {
        let full_path = repo_path.join(target);
        if full_path.exists() {
            std::fs::remove_dir_all(&full_path)?;
            println!("  Removed {}", full_path.display());
            cleaned = true;
        }
    }

    if cleaned {
        println!("\n  Clean complete. Configuration (.snif.json) was not touched.");
    } else {
        println!("  Nothing to clean.");
    }

    Ok(())
}
