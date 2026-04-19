use anyhow::Result;
use snif_config::constants::clean;
use std::path::Path;

fn print_removed(path: &str) {
    println!("{}{}", clean::CLEAN_REMOVED_PREFIX, path);
}

fn print_clean_complete() {
    println!("{}", clean::CLEAN_COMPLETE_MESSAGE);
}

fn print_nothing_to_clean() {
    println!("{}", clean::CLEAN_NOTHING_TO_CLEAN);
}

pub fn run(path: &str) -> Result<()> {
    let repo_path = Path::new(path);
    let mut cleaned = false;

    for target in clean::CLEAN_TARGETS {
        let full_path = repo_path.join(target);
        if full_path.exists() {
            std::fs::remove_dir_all(&full_path)?;
            print_removed(&full_path.display().to_string());
            cleaned = true;
        }
    }

    if cleaned {
        print_clean_complete();
    } else {
        print_nothing_to_clean();
    }

    Ok(())
}
