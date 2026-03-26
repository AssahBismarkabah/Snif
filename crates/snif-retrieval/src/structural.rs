use anyhow::Result;
use snif_store::Store;
use snif_types::{RetrievalMethod, StructuralReason};
use std::collections::HashMap;

pub fn structural_retrieval(
    store: &Store,
    changed_file_ids: &[(i64, String)],
) -> Result<HashMap<i64, Vec<RetrievalMethod>>> {
    let mut results: HashMap<i64, Vec<RetrievalMethod>> = HashMap::new();
    let changed_ids: Vec<i64> = changed_file_ids.iter().map(|(id, _)| *id).collect();

    for (file_id, file_path) in changed_file_ids {
        if let Ok(imports) = store.get_imports_for_file(*file_id) {
            for (imported_id, _) in imports {
                if !changed_ids.contains(&imported_id) {
                    results
                        .entry(imported_id)
                        .or_default()
                        .push(RetrievalMethod::Structural(StructuralReason::DirectImport));
                }
            }
        }

        if let Ok(reverse) = store.get_reverse_imports(file_path) {
            for importer_id in reverse {
                if !changed_ids.contains(&importer_id) {
                    results
                        .entry(importer_id)
                        .or_default()
                        .push(RetrievalMethod::Structural(StructuralReason::ReverseImport));
                }
            }
        }

        if let Ok(cochanges) = store.get_cochange_for_file(*file_id, 0.2) {
            for (other_id, correlation, _) in cochanges {
                if !changed_ids.contains(&other_id) {
                    results
                        .entry(other_id)
                        .or_default()
                        .push(RetrievalMethod::Structural(StructuralReason::CoChange {
                            correlation,
                        }));
                }
            }
        }
    }

    Ok(results)
}
