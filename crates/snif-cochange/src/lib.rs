use anyhow::{Context, Result};
use snif_store::Store;
use std::collections::HashMap;
use std::path::Path;
use std::process::Command;

pub struct CochangeStats {
    pub commits_analyzed: usize,
    pub pairs_stored: usize,
}

pub fn analyze_cochange(
    store: &Store,
    repo_root: &Path,
    min_correlation: f64,
    min_commits: usize,
) -> Result<CochangeStats> {
    let commits = parse_git_log(repo_root)?;
    tracing::debug!(commits = commits.len(), "Parsed git log");

    if commits.is_empty() {
        return Ok(CochangeStats {
            commits_analyzed: 0,
            pairs_stored: 0,
        });
    }

    // Get all indexed file paths for filtering
    let indexed_files: HashMap<String, i64> = store
        .get_all_file_paths()?
        .into_iter()
        .map(|(id, path)| (path, id))
        .collect();

    // Count per-file changes and pair co-occurrences
    let mut file_changes: HashMap<String, usize> = HashMap::new();
    let mut pair_counts: HashMap<(String, String), usize> = HashMap::new();

    for changed_files in &commits {
        let known: Vec<&String> = changed_files
            .iter()
            .filter(|f| indexed_files.contains_key(f.as_str()))
            .collect();

        for f in &known {
            *file_changes.entry((*f).clone()).or_insert(0) += 1;
        }

        for i in 0..known.len() {
            for j in (i + 1)..known.len() {
                let (a, b) = if known[i] < known[j] {
                    (known[i].clone(), known[j].clone())
                } else {
                    (known[j].clone(), known[i].clone())
                };
                *pair_counts.entry((a, b)).or_insert(0) += 1;
            }
        }
    }

    // Compute correlations and filter
    let mut pairs_to_store = Vec::new();

    for ((a, b), co_count) in &pair_counts {
        if *co_count < min_commits {
            continue;
        }

        let changes_a = *file_changes.get(a).unwrap_or(&1) as f64;
        let changes_b = *file_changes.get(b).unwrap_or(&1) as f64;
        let correlation = *co_count as f64 / (changes_a * changes_b).sqrt();

        if correlation >= min_correlation {
            if let (Some(&id_a), Some(&id_b)) = (indexed_files.get(a), indexed_files.get(b)) {
                pairs_to_store.push((id_a, id_b, correlation, *co_count));
            }
        }
    }

    store.delete_all_cochange()?;
    store.insert_cochange_batch(&pairs_to_store)?;

    Ok(CochangeStats {
        commits_analyzed: commits.len(),
        pairs_stored: pairs_to_store.len(),
    })
}

fn parse_git_log(repo_root: &Path) -> Result<Vec<Vec<String>>> {
    let output = Command::new("git")
        .arg("log")
        .arg("--name-only")
        .arg("--format=%H")
        .arg("--no-merges")
        .current_dir(repo_root)
        .output()
        .context("Failed to run git log")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("git log failed: {}", stderr);
    }

    let stdout = String::from_utf8(output.stdout)?;
    let mut commits = Vec::new();
    let mut current_files = Vec::new();
    let mut in_files = false;

    for line in stdout.lines() {
        let line = line.trim();
        if line.is_empty() {
            if !current_files.is_empty() {
                commits.push(std::mem::take(&mut current_files));
            }
            in_files = false;
        } else if line.len() == 40 && line.chars().all(|c| c.is_ascii_hexdigit()) {
            if !current_files.is_empty() {
                commits.push(std::mem::take(&mut current_files));
            }
            in_files = true;
        } else if in_files {
            current_files.push(line.to_string());
        }
    }

    if !current_files.is_empty() {
        commits.push(current_files);
    }

    Ok(commits)
}
