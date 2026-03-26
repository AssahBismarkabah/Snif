pub mod adapter;
pub mod adapters;

use adapter::LanguageAdapter;
use adapters::{python::PythonAdapter, rust::RustAdapter, typescript::TypeScriptAdapter};
use anyhow::Result;
use snif_types::FileExtraction;
use std::path::Path;
use walkdir::WalkDir;

pub use adapter::parse_file;

pub fn all_adapters() -> Vec<Box<dyn LanguageAdapter>> {
    vec![
        Box::new(RustAdapter),
        Box::new(TypeScriptAdapter::new(false)),
        Box::new(TypeScriptAdapter::new(true)),
        Box::new(PythonAdapter),
    ]
}

pub fn detect_adapter(path: &Path) -> Option<Box<dyn LanguageAdapter>> {
    let ext = path.extension()?.to_str()?;
    for a in all_adapters() {
        if a.file_extensions().contains(&ext) {
            return Some(a);
        }
    }
    None
}

pub fn parse_repository(
    root: &Path,
    exclude_patterns: &[String],
) -> Result<Vec<FileExtraction>> {
    let mut extractions = Vec::new();

    for entry in WalkDir::new(root)
        .into_iter()
        .filter_entry(|e| {
            let name = e.file_name().to_str().unwrap_or("");
            // Allow the root directory itself, filter hidden dirs and excluded patterns
            e.depth() == 0
                || (!name.starts_with('.') && !exclude_patterns.iter().any(|p| p == name))
        })
    {
        let entry = entry?;
        if !entry.file_type().is_file() {
            continue;
        }

        let path = entry.path();
        let a = match detect_adapter(path) {
            Some(a) => a,
            None => continue,
        };

        let source = match std::fs::read(path) {
            Ok(s) => s,
            Err(_) => continue,
        };

        if source.len() > 1_000_000 || !is_likely_text(&source) {
            continue;
        }

        let rel_path = path.strip_prefix(root).unwrap_or(path);
        match adapter::parse_file(a.as_ref(), &rel_path.to_string_lossy(), &source) {
            Ok(extraction) => extractions.push(extraction),
            Err(e) => {
                tracing::warn!(path = %rel_path.display(), error = %e, "Failed to parse file");
            }
        }
    }

    tracing::info!(files = extractions.len(), "Parsed repository");
    Ok(extractions)
}

fn is_likely_text(data: &[u8]) -> bool {
    let check_len = data.len().min(512);
    !data[..check_len].contains(&0)
}
