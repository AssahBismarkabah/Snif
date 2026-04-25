use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use snif_config::constants::eval_output;
use std::collections::HashMap;
use std::path::Path;
use walkdir::WalkDir;

#[derive(Debug, Clone)]
pub struct Fixture {
    pub name: String,
    pub description: String,
    pub diff: String,
    pub files: HashMap<String, String>,
    pub conventions: Option<String>,
    pub expected_findings: Vec<ExpectedFinding>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExpectedFinding {
    pub file: String,
    pub start_line: usize,
    pub category: String,
    /// Alternative categories that should also be accepted as a match.
    /// Useful when the bug is genuinely ambiguous (e.g., resource leak
    /// could be "logic" or "performance"; unsafe type assertion could
    /// be "security" or "logic").
    #[serde(default)]
    pub acceptable_categories: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct FixtureMeta {
    name: String,
    description: String,
    conventions: Option<String>,
    expected_findings: Vec<ExpectedFinding>,
}

pub fn load_fixtures(dir: &Path) -> Result<Vec<Fixture>> {
    let mut fixtures = Vec::new();

    for entry in std::fs::read_dir(dir)
        .with_context(|| format!("Failed to read fixtures directory: {}", dir.display()))?
    {
        let entry = entry?;
        if !entry.file_type()?.is_dir() {
            continue;
        }

        let fixture_dir = entry.path();
        let meta_path = fixture_dir.join(eval_output::FIXTURE_META_FILE);

        if !meta_path.exists() {
            continue;
        }

        let fixture = load_single_fixture(&fixture_dir)
            .with_context(|| format!("Failed to load fixture: {}", fixture_dir.display()))?;

        fixtures.push(fixture);
    }

    fixtures.sort_by(|a, b| a.name.cmp(&b.name));
    tracing::info!(count = fixtures.len(), "Loaded fixtures");
    Ok(fixtures)
}

fn load_single_fixture(fixture_dir: &Path) -> Result<Fixture> {
    let meta_path = fixture_dir.join(eval_output::FIXTURE_META_FILE);
    let meta_content = std::fs::read_to_string(&meta_path)
        .with_context(|| format!("Failed to read {}", meta_path.display()))?;
    let meta: FixtureMeta = serde_json::from_str(&meta_content)
        .with_context(|| format!("Failed to parse {}", meta_path.display()))?;

    let patch_path = fixture_dir.join(eval_output::PATCH_FILE);
    let diff = std::fs::read_to_string(&patch_path)
        .with_context(|| format!("Failed to read {}", patch_path.display()))?;

    let mut files = HashMap::new();
    for entry in WalkDir::new(fixture_dir) {
        let entry = entry?;
        if !entry.file_type().is_file() {
            continue;
        }

        let path = entry.path();
        let name = path.file_name().unwrap_or_default().to_string_lossy();

        if name == eval_output::FIXTURE_META_FILE || name == eval_output::PATCH_FILE {
            continue;
        }

        let rel_path = path
            .strip_prefix(fixture_dir)
            .unwrap_or(path)
            .to_string_lossy()
            .to_string();

        let content = std::fs::read_to_string(path)
            .with_context(|| format!("Failed to read {}", path.display()))?;

        files.insert(rel_path, content);
    }

    Ok(Fixture {
        name: meta.name,
        description: meta.description,
        diff,
        files,
        conventions: meta.conventions,
        expected_findings: meta.expected_findings,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::TempDir;

    const TEST_FIXTURE_NAME: &str = "test-fixture";
    const TEST_FIXTURE_DESCRIPTION: &str = "Test fixture for loading";

    #[test]
    fn load_fixtures_returns_empty_for_empty_dir() {
        let temp_dir = TempDir::new().unwrap();
        let fixtures = load_fixtures(temp_dir.path()).unwrap();
        assert!(fixtures.is_empty());
    }

    #[test]
    fn load_fixtures_loads_valid_fixture() {
        let temp_dir = TempDir::new().unwrap();
        let fixture_dir = temp_dir.path().join(TEST_FIXTURE_NAME);
        std::fs::create_dir_all(&fixture_dir).unwrap();

        let meta = serde_json::json!({
            "name": TEST_FIXTURE_NAME,
            "description": TEST_FIXTURE_DESCRIPTION,
            "expected_findings": []
        });
        let mut meta_file =
            std::fs::File::create(fixture_dir.join(eval_output::FIXTURE_META_FILE)).unwrap();
        meta_file.write_all(meta.to_string().as_bytes()).unwrap();

        let mut patch_file =
            std::fs::File::create(fixture_dir.join(eval_output::PATCH_FILE)).unwrap();
        patch_file.write_all(b"diff content").unwrap();

        let fixtures = load_fixtures(temp_dir.path()).unwrap();
        assert_eq!(fixtures.len(), 1);
        assert_eq!(fixtures[0].name, TEST_FIXTURE_NAME);
    }

    #[test]
    fn load_fixtures_skips_dirs_without_meta() {
        let temp_dir = TempDir::new().unwrap();
        std::fs::create_dir_all(temp_dir.path().join("no-meta")).unwrap();

        let fixtures = load_fixtures(temp_dir.path()).unwrap();
        assert!(fixtures.is_empty());
    }
}
