use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
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
        let meta_path = fixture_dir.join("fixture.json");

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
    let meta_path = fixture_dir.join("fixture.json");
    let meta_content = std::fs::read_to_string(&meta_path)
        .with_context(|| format!("Failed to read {}", meta_path.display()))?;
    let meta: FixtureMeta = serde_json::from_str(&meta_content)
        .with_context(|| format!("Failed to parse {}", meta_path.display()))?;

    let patch_path = fixture_dir.join("change.patch");
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

        if name == "fixture.json" || name == "change.patch" {
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
