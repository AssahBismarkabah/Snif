use anyhow::{Context, Result};
use fs2::FileExt;
use serde::{Deserialize, Serialize};
use snif_config::constants::{eval_output, thresholds, time};
use std::io::{BufRead, Write};
use std::path::Path;

use crate::metrics::{AggregateMetrics, FixtureResult};

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
    if precision_drop > thresholds::PRECISION_REGRESSION_THRESHOLD {
        let pct_drop = precision_drop * eval_output::PERCENTAGE_MULTIPLIER;
        let prev_pct = previous.precision * eval_output::PERCENTAGE_MULTIPLIER;
        let curr_pct = current.precision * eval_output::PERCENTAGE_MULTIPLIER;
        warnings.push(RegressionWarning {
            message: format!(
                "Precision dropped {:.1}pp ({:.1}% -> {:.1}%)",
                pct_drop, prev_pct, curr_pct,
            ),
        });
    }

    let recall_drop = previous.recall - current.recall;
    if recall_drop > thresholds::RECALL_REGRESSION_THRESHOLD {
        let recall_pct_drop = recall_drop * eval_output::PERCENTAGE_MULTIPLIER;
        let prev_recall_pct = previous.recall * eval_output::PERCENTAGE_MULTIPLIER;
        let curr_recall_pct = current.recall * eval_output::PERCENTAGE_MULTIPLIER;
        warnings.push(RegressionWarning {
            message: format!(
                "Recall dropped {:.1}pp ({:.1}% -> {:.1}%)",
                recall_pct_drop, prev_recall_pct, curr_recall_pct,
            ),
        });
    }

    let noise_increase = current.noise_rate - previous.noise_rate;
    if noise_increase > thresholds::NOISE_REGRESSION_THRESHOLD {
        let noise_pct_increase = noise_increase * eval_output::PERCENTAGE_MULTIPLIER;
        let prev_noise_pct = previous.noise_rate * eval_output::PERCENTAGE_MULTIPLIER;
        let curr_noise_pct = current.noise_rate * eval_output::PERCENTAGE_MULTIPLIER;
        warnings.push(RegressionWarning {
            message: format!(
                "Noise rate increased {:.1}pp ({:.1}% -> {:.1}%)",
                noise_pct_increase, prev_noise_pct, curr_noise_pct,
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
            if prev_fix.tp > eval_output::DEFAULT_COUNTER
                && curr_fix.tp == eval_output::DEFAULT_COUNTER
            {
                warnings.push(RegressionWarning {
                    message: format!(
                        "Fixture '{}': lost detection (TP {} -> 0)",
                        curr_fix.name, prev_fix.tp,
                    ),
                });
            }

            if prev_fix.fp == eval_output::DEFAULT_COUNTER
                && curr_fix.fp > eval_output::DEFAULT_COUNTER
            {
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
    let days = secs / time::SECS_PER_DAY;
    let time_secs = secs % time::SECS_PER_DAY;
    let hours = time_secs / time::SECS_PER_HOUR;
    let minutes = (time_secs % time::SECS_PER_HOUR) / time::SECS_PER_MINUTE;
    let seconds = time_secs % time::SECS_PER_MINUTE;

    // Days since epoch to year/month/day
    let (year, month, day) = days_to_ymd(days);

    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z",
        year, month, day, hours, minutes, seconds
    )
}

fn days_to_ymd(mut days: u64) -> (u64, u64, u64) {
    // Algorithm from http://howardhinnant.github.io/date_algorithms.html
    days += 719_468;
    let era = days / 146_097;
    let doe = days - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
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
        .unwrap_or_else(|| eval_output::UNKNOWN_GIT_SHA.to_string())
}

#[cfg(test)]
mod tests {
    use super::{check_regression, load_history, save_record, EvalRecord, FixtureRecord};
    use snif_config::constants::eval_output;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    const TEST_TIMESTAMP: &str = "2026-04-09T00:00:00Z";
    const TEST_FIXTURE_NAME: &str = "fixture-a";
    const TEST_FIXTURE_EXPECTED: usize = 1;

    const TEST_SHA_1: &str = "sha-1";
    const TEST_SHA_2: &str = "sha-2";

    const TEST_PRECISION_HIGH: f64 = 0.90;
    const TEST_RECALL_HIGH: f64 = 0.95;
    const TEST_NOISE_LOW: f64 = 0.05;
    const TEST_TP_HIGH: usize = 1;
    const TEST_FP_ZERO: usize = 0;

    const TEST_PRECISION_MID_1: f64 = 0.82;
    const TEST_RECALL_MID: f64 = 0.90;
    const TEST_NOISE_VERY_LOW: f64 = 0.08;

    const TEST_PRECISION_MID_2: f64 = 0.79;
    const TEST_NOISE_LOW_2: f64 = 0.10;

    const TEST_PRECISION_LOW: f64 = 0.80;
    const TEST_RECALL_LOW: f64 = 0.80;
    const TEST_NOISE_HIGH: f64 = 0.12;
    const TEST_TP_ZERO: usize = 0;
    const TEST_FP_MID: usize = 2;

    fn sample_record(
        git_sha: &str,
        precision: f64,
        recall: f64,
        noise_rate: f64,
        tp: usize,
        fp: usize,
    ) -> EvalRecord {
        EvalRecord {
            timestamp: TEST_TIMESTAMP.to_string(),
            git_sha: git_sha.to_string(),
            fixture_count: TEST_FIXTURE_EXPECTED,
            precision,
            recall,
            noise_rate,
            gates_passed: true,
            per_fixture: vec![FixtureRecord {
                name: TEST_FIXTURE_NAME.to_string(),
                expected: TEST_FIXTURE_EXPECTED,
                actual: tp + fp,
                tp,
                fp,
                fn_count: usize::from(tp == eval_output::DEFAULT_COUNTER),
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

    #[test]
    fn load_history_returns_empty_for_missing_file() {
        let path = unique_history_path("missing");
        let records = load_history(&path).expect("missing history should be treated as empty");
        assert!(records.is_empty());
    }

    #[test]
    fn save_and_load_history_round_trip() {
        let path = unique_history_path("round-trip");
        let first = sample_record(
            TEST_SHA_1,
            TEST_PRECISION_MID_1,
            TEST_RECALL_MID,
            TEST_NOISE_VERY_LOW,
            TEST_TP_HIGH,
            TEST_FP_ZERO,
        );
        let second = sample_record(
            TEST_SHA_2,
            TEST_PRECISION_MID_2,
            TEST_RECALL_MID,
            TEST_NOISE_LOW_2,
            TEST_TP_HIGH,
            TEST_TP_HIGH,
        );

        save_record(&path, &first).expect("first record should save");
        save_record(&path, &second).expect("second record should save");

        let loaded = load_history(&path).expect("history should load");
        assert_eq!(loaded.len(), 2);
        assert_eq!(loaded[0].git_sha, TEST_SHA_1);
        assert_eq!(loaded[1].git_sha, TEST_SHA_2);

        std::fs::remove_file(path).expect("test history should be removed");
    }

    #[test]
    fn regression_detection_reports_aggregate_and_fixture_changes() {
        let previous = sample_record(
            TEST_SHA_1,
            TEST_PRECISION_HIGH,
            TEST_RECALL_HIGH,
            TEST_NOISE_LOW,
            TEST_TP_HIGH,
            TEST_FP_ZERO,
        );
        let current = sample_record(
            TEST_SHA_2,
            TEST_PRECISION_LOW,
            TEST_RECALL_LOW,
            TEST_NOISE_HIGH,
            TEST_TP_ZERO,
            TEST_FP_MID,
        );

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
}
