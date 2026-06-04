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
    let repo_root = repo_path
        .canonicalize()
        .unwrap_or_else(|_| repo_path.to_path_buf());
    let mut cleaned = false;

    for target in clean::CLEAN_TARGETS {
        let full_path = repo_path.join(target);
        if full_path.exists() {
            std::fs::remove_dir_all(&full_path)?;
            print_removed(&full_path.display().to_string());
            cleaned = true;
        }
    }

    let config = snif_config::SnifConfig::load(repo_path).unwrap_or_else(|error| {
        tracing::warn!(
            error = %error,
            "Failed to load .snif.json while cleaning; using default embedding cache path"
        );
        snif_config::SnifConfig::default_with_env()
    });
    let embedding_cache_dir = config.resolved_embedding_cache_dir(repo_path);
    if embedding_cache_dir.exists() {
        if is_inside_repo(&repo_root, &embedding_cache_dir)? {
            std::fs::remove_dir_all(&embedding_cache_dir)?;
            print_removed(&embedding_cache_dir.display().to_string());
            cleaned = true;
        } else {
            tracing::info!(
                cache_dir = %embedding_cache_dir.display(),
                "Skipping embedding cache outside repository root"
            );
        }
    }

    if cleaned {
        print_clean_complete();
    } else {
        print_nothing_to_clean();
    }

    Ok(())
}

fn is_inside_repo(repo_root: &Path, path: &Path) -> Result<bool> {
    let canonical_path = path.canonicalize()?;
    Ok(canonical_path != repo_root && canonical_path.starts_with(repo_root))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn clean_does_not_delete_shared_absolute_embedding_cache() {
        let _guard = env_lock().lock().expect("env lock should not be poisoned");
        let old_env = clear_embedding_cache_env();
        let root = temp_path("snif-clean-repo");
        let shared_cache = temp_path("snif-shared-fastembed-cache");
        std::fs::create_dir_all(root.join(".snif")).expect("repo data should be created");
        std::fs::create_dir_all(&shared_cache).expect("shared cache should be created");
        std::fs::write(
            root.join(".snif.json"),
            format!(
                r#"{{
                    "index": {{
                        "embedding_cache_dir": "{}"
                    }}
                }}"#,
                shared_cache.display()
            ),
        )
        .expect("config should be written");

        run(root.to_str().expect("temp path should be utf8")).expect("clean should succeed");

        assert!(!root.join(".snif").exists());
        assert!(shared_cache.exists());

        let _ = std::fs::remove_dir_all(&root);
        let _ = std::fs::remove_dir_all(&shared_cache);
        restore_embedding_cache_env(old_env);
    }

    #[test]
    fn clean_removes_runtime_data_when_config_is_invalid() {
        let _guard = env_lock().lock().expect("env lock should not be poisoned");
        let old_env = clear_embedding_cache_env();
        let root = temp_path("snif-clean-invalid-config");
        std::fs::create_dir_all(root.join(".snif")).expect("repo data should be created");
        std::fs::create_dir_all(root.join(".fastembed_cache"))
            .expect("embedding cache should be created");
        std::fs::write(root.join(".snif.json"), "{ invalid json")
            .expect("invalid config should be written");

        run(root.to_str().expect("temp path should be utf8")).expect("clean should succeed");

        assert!(!root.join(".snif").exists());
        assert!(!root.join(".fastembed_cache").exists());

        let _ = std::fs::remove_dir_all(&root);
        restore_embedding_cache_env(old_env);
    }

    #[test]
    fn clean_honors_env_cache_path_when_config_is_invalid() {
        let _guard = env_lock().lock().expect("env lock should not be poisoned");
        let old_env = clear_embedding_cache_env();

        let root = temp_path("snif-clean-invalid-config-env");
        std::fs::create_dir_all(root.join(".snif")).expect("repo data should be created");
        std::fs::create_dir_all(root.join(".custom-fastembed-cache"))
            .expect("embedding cache should be created");
        std::fs::write(root.join(".snif.json"), "{ invalid json")
            .expect("invalid config should be written");

        std::env::set_var(
            snif_config::env::app::SNIF_EMBEDDING_CACHE_DIR,
            ".custom-fastembed-cache",
        );

        run(root.to_str().expect("temp path should be utf8")).expect("clean should succeed");

        assert!(!root.join(".snif").exists());
        assert!(!root.join(".custom-fastembed-cache").exists());

        restore_embedding_cache_env(old_env);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn clean_does_not_delete_repo_root_when_cache_points_to_root() {
        let _guard = env_lock().lock().expect("env lock should not be poisoned");
        let old_env = clear_embedding_cache_env();
        let root = temp_path("snif-clean-cache-root");
        std::fs::create_dir_all(root.join(".snif")).expect("repo data should be created");
        std::fs::write(
            root.join(".snif.json"),
            r#"{
                "index": {
                    "embedding_cache_dir": "."
                }
            }"#,
        )
        .expect("config should be written");

        run(root.to_str().expect("temp path should be utf8")).expect("clean should succeed");

        assert!(root.exists());
        assert!(root.join(".snif.json").exists());
        assert!(!root.join(".snif").exists());

        let _ = std::fs::remove_dir_all(&root);
        restore_embedding_cache_env(old_env);
    }

    fn temp_path(prefix: &str) -> PathBuf {
        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time should be after epoch")
            .as_nanos();
        std::env::temp_dir().join(format!("{}-{}-{}", prefix, std::process::id(), suffix))
    }

    fn env_lock() -> &'static std::sync::Mutex<()> {
        static LOCK: std::sync::OnceLock<std::sync::Mutex<()>> = std::sync::OnceLock::new();
        LOCK.get_or_init(|| std::sync::Mutex::new(()))
    }

    fn restore_env_var(key: &str, old_value: Option<String>) {
        match old_value {
            Some(value) => std::env::set_var(key, value),
            None => std::env::remove_var(key),
        }
    }

    fn clear_embedding_cache_env() -> (Option<String>, Option<String>) {
        let old_fastembed = std::env::var(snif_config::env::app::FASTEMBED_CACHE_DIR).ok();
        let old_snif = std::env::var(snif_config::env::app::SNIF_EMBEDDING_CACHE_DIR).ok();
        std::env::remove_var(snif_config::env::app::FASTEMBED_CACHE_DIR);
        std::env::remove_var(snif_config::env::app::SNIF_EMBEDDING_CACHE_DIR);
        (old_fastembed, old_snif)
    }

    fn restore_embedding_cache_env(old_env: (Option<String>, Option<String>)) {
        restore_env_var(snif_config::env::app::FASTEMBED_CACHE_DIR, old_env.0);
        restore_env_var(snif_config::env::app::SNIF_EMBEDDING_CACHE_DIR, old_env.1);
    }
}
