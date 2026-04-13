use anyhow::{Context, Result};
use fs2::FileExt;
use serde::{Deserialize, Serialize};
use std::io::{BufRead, Write};
use std::path::Path;

use crate::metrics::{AggregateMetrics, FixtureResult};

/// Precision drop threshold: flag regression if precision falls more than this.
const PRECISION_REGRESSION_THRESHOLD: f64 = 0.05;

/// Recall drop threshold: flag regression if recall falls more than this.
const RECALL_REGRESSION_THRESHOLD: f64 = 0.10;

/// Noise increase threshold: flag regression if noise rate rises more than this.
const NOISE_REGRESSION_THRESHOLD: f64 = 0.05;

#[derive(Debug, Serialize, Deserialize)]
pub struct EvalRecord {
    pub timestamp: String,
    pub git_sha: String,
    pub fixture_count: usize,
    pub precision: f64,
    pub recall: f64,
    pub noise_rate: f64,
    pub gates_passed: bool,
    pub per_fixture: Vec<FixtureRecord>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct FixtureRecord {
    pub name: String,
    pub expected: usize,
    pub actual: usize,
    pub tp: usize,
    pub fp: usize,
    pub fn_count: usize,
}

pub struct RegressionWarning {
    pub message: String,
}

pub fn build_record(
    fixture_results: &[FixtureResult],
    aggregate: &AggregateMetrics,
    gates_passed: bool,
) -> EvalRecord {
    let timestamp = iso8601_now();
    let git_sha = current_git_sha();

    let per_fixture = fixture_results
        .iter()
        .map(|fr| FixtureRecord {
            name: fr.fixture_name.clone(),
            expected: fr.expected,
            actual: fr.actual,
            tp: fr.true_positives,
            fp: fr.false_positives,
            fn_count: fr.false_negatives,
        })
        .collect();

    EvalRecord {
        timestamp,
        git_sha,
        fixture_count: fixture_results.len(),
        precision: aggregate.precision,
        recall: aggregate.recall,
        noise_rate: aggregate.noise_rate,
        gates_passed,
        per_fixture,
    }
}

pub fn save_record(path: &Path, record: &EvalRecord) -> Result<()> {
    let line = serde_json::to_string(record).context("Failed to serialize eval record")?;

    if let Some(parent) = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        std::fs::create_dir_all(parent).with_context(|| {
            format!("Failed to create history directory for {}", path.display())
        })?;
    }

    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .with_context(|| format!("Failed to open history file: {}", path.display()))?;

    file.lock_exclusive()
        .with_context(|| format!("Failed to lock history file: {}", path.display()))?;
    writeln!(file, "{}", line).context("Failed to write eval record")?;
    file.unlock()
        .with_context(|| "Failed to unlock history file")?;

    tracing::info!(path = %path.display(), "Eval record saved to history");
    Ok(())
}

pub fn load_history(path: &Path) -> Result<Vec<EvalRecord>> {
    if !path.exists() {
        return Ok(Vec::new());
    }

    let file =
        std::fs::File::open(path).with_context(|| format!("Failed to open {}", path.display()))?;

    let reader = std::io::BufReader::new(file);
    let mut records = Vec::new();

    for (i, line) in reader.lines().enumerate() {
        let line = line.with_context(|| format!("Failed to read line {} of history", i + 1))?;
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let record: EvalRecord = serde_json::from_str(trimmed)
            .with_context(|| format!("Failed to parse history line {}", i + 1))?;
        records.push(record);
    }

    Ok(records)
}

pub fn check_regression(current: &EvalRecord, previous: &EvalRecord) -> Vec<RegressionWarning> {
    let mut warnings = Vec::new();

    // Aggregate metric regressions
    let precision_drop = previous.precision - current.precision;
    if precision_drop > PRECISION_REGRESSION_THRESHOLD {
        warnings.push(RegressionWarning {
            message: format!(
                "Precision dropped {:.1}pp ({:.1}% -> {:.1}%)",
                precision_drop * 100.0,
                previous.precision * 100.0,
                current.precision * 100.0,
            ),
        });
    }

    let recall_drop = previous.recall - current.recall;
    if recall_drop > RECALL_REGRESSION_THRESHOLD {
        warnings.push(RegressionWarning {
            message: format!(
                "Recall dropped {:.1}pp ({:.1}% -> {:.1}%)",
                recall_drop * 100.0,
                previous.recall * 100.0,
                current.recall * 100.0,
            ),
        });
    }

    let noise_increase = current.noise_rate - previous.noise_rate;
    if noise_increase > NOISE_REGRESSION_THRESHOLD {
        warnings.push(RegressionWarning {
            message: format!(
                "Noise rate increased {:.1}pp ({:.1}% -> {:.1}%)",
                noise_increase * 100.0,
                previous.noise_rate * 100.0,
                current.noise_rate * 100.0,
            ),
        });
    }

    // Per-fixture regressions: lost detections and new false positives
    let prev_by_name: std::collections::HashMap<&str, &FixtureRecord> = previous
        .per_fixture
        .iter()
        .map(|f| (f.name.as_str(), f))
        .collect();

    for curr_fix in &current.per_fixture {
        if let Some(prev_fix) = prev_by_name.get(curr_fix.name.as_str()) {
            if prev_fix.tp > 0 && curr_fix.tp == 0 {
                warnings.push(RegressionWarning {
                    message: format!(
                        "Fixture '{}': lost detection (TP {} -> 0)",
                        curr_fix.name, prev_fix.tp,
                    ),
                });
            }

            if prev_fix.fp == 0 && curr_fix.fp > 0 {
                warnings.push(RegressionWarning {
                    message: format!(
                        "Fixture '{}': new false positives (FP 0 -> {})",
                        curr_fix.name, curr_fix.fp,
                    ),
                });
            }
        }
    }

    warnings
}

fn iso8601_now() -> String {
    let now = std::time::SystemTime::now();
    let duration = now
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    let secs = duration.as_secs();

    // Convert to date/time components
    let days = secs / 86400;
    let time_secs = secs % 86400;
    let hours = time_secs / 3600;
    let minutes = (time_secs % 3600) / 60;
    let seconds = time_secs % 60;

    // Days since epoch to year/month/day
    let (year, month, day) = days_to_ymd(days);

    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z",
        year, month, day, hours, minutes, seconds
    )
}

fn days_to_ymd(mut days: u64) -> (u64, u64, u64) {
    // Algorithm from http://howardhinnant.github.io/date_algorithms.html
    days += 719468;
    let era = days / 146097;
    let doe = days - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    (y, m, d)
}

fn current_git_sha() -> String {
    std::process::Command::new("git")
        .args(["rev-parse", "HEAD"])
        .output()
        .ok()
        .and_then(|o| {
            if o.status.success() {
                String::from_utf8(o.stdout)
                    .ok()
                    .map(|s| s.trim().to_string())
            } else {
                None
            }
        })
        .unwrap_or_else(|| "unknown".to_string())
}

#[cfg(test)]
mod tests {
    use super::{check_regression, load_history, save_record, EvalRecord, FixtureRecord};
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn load_history_returns_empty_for_missing_file() {
        let path = unique_history_path("missing");
        let records = load_history(&path).expect("missing history should be treated as empty");
        assert!(records.is_empty());
    }

    #[test]
    fn save_and_load_history_round_trip() {
        let path = unique_history_path("round-trip");
        let first = sample_record("sha-1", 0.82, 0.90, 0.08, 1, 0);
        let second = sample_record("sha-2", 0.79, 0.88, 0.10, 1, 1);

        save_record(&path, &first).expect("first record should save");
        save_record(&path, &second).expect("second record should save");

        let loaded = load_history(&path).expect("history should load");
        assert_eq!(loaded.len(), 2);
        assert_eq!(loaded[0].git_sha, "sha-1");
        assert_eq!(loaded[1].git_sha, "sha-2");

        std::fs::remove_file(path).expect("test history should be removed");
    }

    #[test]
    fn regression_detection_reports_aggregate_and_fixture_changes() {
        let previous = sample_record("sha-1", 0.90, 0.95, 0.05, 1, 0);
        let current = sample_record("sha-2", 0.80, 0.80, 0.12, 0, 2);

        let warnings = check_regression(&current, &previous);
        let messages: Vec<&str> = warnings
            .iter()
            .map(|warning| warning.message.as_str())
            .collect();

        assert!(
            messages
                .iter()
                .any(|message| message.contains("Precision dropped")),
            "expected precision warning"
        );
        assert!(
            messages
                .iter()
                .any(|message| message.contains("Recall dropped")),
            "expected recall warning"
        );
        assert!(
            messages
                .iter()
                .any(|message| message.contains("Noise rate increased")),
            "expected noise warning"
        );
        assert!(
            messages
                .iter()
                .any(|message| message.contains("lost detection")),
            "expected lost detection warning"
        );
        assert!(
            messages
                .iter()
                .any(|message| message.contains("new false positives")),
            "expected false positive warning"
        );
    }

    fn sample_record(
        git_sha: &str,
        precision: f64,
        recall: f64,
        noise_rate: f64,
        tp: usize,
        fp: usize,
    ) -> EvalRecord {
        EvalRecord {
            timestamp: "2026-04-09T00:00:00Z".to_string(),
            git_sha: git_sha.to_string(),
            fixture_count: 1,
            precision,
            recall,
            noise_rate,
            gates_passed: true,
            per_fixture: vec![FixtureRecord {
                name: "fixture-a".to_string(),
                expected: 1,
                actual: tp + fp,
                tp,
                fp,
                fn_count: usize::from(tp == 0),
            }],
        }
    }

    fn unique_history_path(label: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock should be after unix epoch")
            .as_nanos();

        std::env::temp_dir().join(format!("snif-eval-history-{label}-{nanos}.jsonl"))
    }
}
