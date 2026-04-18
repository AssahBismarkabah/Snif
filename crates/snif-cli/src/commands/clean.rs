use anyhow::Result;
use snif_config::constants::clean;
use std::path::Path;

pub fn run(path: &str) -> Result<()> {
    let repo_path = Path::new(path);

    let mut cleaned = false;

    for target in clean::CLEAN_TARGETS {
        let full_path = repo_path.join(target);
        if full_path.exists() {
            std::fs::remove_dir_all(&full_path)?;
            println!("{}{}", clean::CLEAN_REMOVED_PREFIX, full_path.display());
            cleaned = true;
        }
    }

    if cleaned {
        println!("{}", clean::CLEAN_COMPLETE_MESSAGE);
    } else {
        println!("{}", clean::CLEAN_NOTHING_TO_CLEAN);
    }

    Ok(())
}
