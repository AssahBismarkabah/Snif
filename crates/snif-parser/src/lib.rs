pub mod adapter;
pub mod adapters;

use adapter::LanguageAdapter;
use adapters::{
    java::JavaAdapter, python::PythonAdapter, rust::RustAdapter, typescript::TypeScriptAdapter,
};
use anyhow::Result;
use rayon::prelude::*;
use snif_config::constants::limits;
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
        Box::new(JavaAdapter),
    ]
}

pub fn detect_adapter(path: &Path) -> Option<Box<dyn LanguageAdapter>> {
    let ext = path.extension()?.to_str()?;
    all_adapters()
        .into_iter()
        .find(|a| a.file_extensions().contains(&ext))
}

pub fn parse_repository(root: &Path, exclude_patterns: &[String]) -> Result<Vec<FileExtraction>> {
    let entries: Vec<_> = WalkDir::new(root)
        .into_iter()
        .filter_entry(|e| {
            let name = e.file_name().to_str().unwrap_or("");
            e.depth() == 0
                || (!name.starts_with('.') && !exclude_patterns.iter().any(|p| p == name))
        })
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
        .collect();

    let extractions: Vec<_> = entries
        .par_iter()
        .filter_map(|entry| {
            let path = entry.path();
            let a = match detect_adapter(path) {
                Some(a) => a,
                None => return None,
            };

            let source = match std::fs::read(path) {
                Ok(s) => s,
                Err(_) => return None,
            };

            if source.len() > limits::MAX_FILE_SIZE_BYTES || !is_likely_text(&source) {
                return None;
            }

            let rel_path = path.strip_prefix(root).unwrap_or(path);
            match adapter::parse_file(a.as_ref(), &rel_path.to_string_lossy(), &source) {
                Ok(extraction) => Some(extraction),
                Err(e) => {
                    tracing::warn!(path = %rel_path.display(), error = %e, "Failed to parse file");
                    None
                }
            }
        })
        .collect();

    tracing::info!(files = extractions.len(), "Parsed repository");
    Ok(extractions)
}

fn is_likely_text(data: &[u8]) -> bool {
    let check_len = data.len().min(limits::TEXT_DETECTION_SAMPLE_SIZE);
    !data[..check_len].contains(&0)
}
