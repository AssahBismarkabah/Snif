use anyhow::Result;
use sha2::{Digest, Sha256};
use snif_store::Store;
use snif_types::FileExtraction;

pub struct GraphStats {
    pub files_indexed: usize,
    pub files_skipped: usize,
    pub symbols_extracted: usize,
    pub imports_extracted: usize,
    pub references_extracted: usize,
}

pub fn build_graph(store: &Store, extractions: &[FileExtraction]) -> Result<GraphStats> {
    let mut stats = GraphStats {
        files_indexed: 0,
        files_skipped: 0,
        symbols_extracted: 0,
        imports_extracted: 0,
        references_extracted: 0,
    };

    for extraction in extractions {
        let content_hash = compute_hash(&extraction);

        // Check if file is already indexed with the same hash
        if let Some(existing_hash) = store.get_file_hash(&extraction.path)? {
            if existing_hash == content_hash {
                stats.files_skipped += 1;
                continue;
            }
        }

        let file_id = store.upsert_file(
            &extraction.path,
            &content_hash,
            &extraction.language.to_string(),
        )?;

        // Clear old data for this file
        store.delete_symbols_for_file(file_id)?;
        store.delete_imports_for_file(file_id)?;
        store.delete_refs_for_file(file_id)?;

        // Insert new data
        store.insert_symbols(file_id, &extraction.symbols)?;
        store.insert_imports(file_id, &extraction.imports)?;
        store.insert_refs(file_id, &extraction.references)?;

        stats.files_indexed += 1;
        stats.symbols_extracted += extraction.symbols.len();
        stats.imports_extracted += extraction.imports.len();
        stats.references_extracted += extraction.references.len();
    }

    Ok(stats)
}

fn compute_hash(extraction: &FileExtraction) -> String {
    let mut hasher = Sha256::new();
    hasher.update(extraction.path.as_bytes());
    for sym in &extraction.symbols {
        hasher.update(sym.name.as_bytes());
        hasher.update(sym.kind.to_string().as_bytes());
    }
    for imp in &extraction.imports {
        hasher.update(imp.source.as_bytes());
    }
    format!("{:x}", hasher.finalize())
}
