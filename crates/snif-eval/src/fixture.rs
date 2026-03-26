use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;
use walkdir::WalkDir;

#[derive(Debug, Clone, Serialize, Deserialize)]
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
    pub description: String,
}

pub fn load_fixtures(dir: &Path) -> Result<Vec<Fixture>> {
    let mut fixtures = Vec::new();

    for entry in WalkDir::new(dir).max_depth(1) {
        let entry = entry?;
        if !entry.file_type().is_file() {
            continue;
        }

        let path = entry.path();
        if path.extension().map_or(true, |e| e != "json") {
            continue;
        }

        let content = std::fs::read_to_string(path)
            .with_context(|| format!("Failed to read fixture: {}", path.display()))?;

        let fixture: Fixture = serde_json::from_str(&content)
            .with_context(|| format!("Failed to parse fixture: {}", path.display()))?;

        fixtures.push(fixture);
    }

    fixtures.sort_by(|a, b| a.name.cmp(&b.name));
    tracing::info!(count = fixtures.len(), "Loaded fixtures");
    Ok(fixtures)
}
