use anyhow::{Context, Result};
use reqwest::blocking::Client;
use serde_json::json;

use crate::history::EvalRecord;
use crate::metrics::FixtureResult;

/// Braintrust API base URL.
const BRAINTRUST_API_BASE: &str = "https://api.braintrust.dev";

/// HTTP request timeout for API calls.
const HTTP_TIMEOUT_SECS: u64 = 15;

/// Default Braintrust project ID.
/// Override with the `SNIF_BRAINTRUST_PROJECT_ID` environment variable in CI/CD.
pub const BRAINTRUST_DEFAULT_PROJECT_ID: &str = "7c476f2d-a083-4eb2-bd93-430266782cd0";

/// Stable experiment name. All eval runs insert into the same experiment,
/// allowing trend tracking over time. Individual runs are distinguished by
/// metadata (git_sha, timestamp) on each event.
const EXPERIMENT_NAME: &str = "snif-eval";

/// Human-readable description for the experiment in the Braintrust dashboard.
const EXPERIMENT_DESCRIPTION: &str = "Snif eval harness results";

/// Tag applied to all experiments from this eval harness.
const EVAL_TAG: &str = "snif-eval";

/// Tag applied when quality gates pass; inverted-gates tag used otherwise.
const GATES_PASSED_TAG: &str = "gates-passed";
const GATES_FAILED_TAG: &str = "gates-failed";

/// F1 score coefficient (2.0 for harmonic mean of precision and recall).
const F1_COEFFICIENT: f64 = 2.0;

/// Default score values when a fixture has no findings to evaluate.
const DEFAULT_SCORE_WHEN_NO_DATA: f64 = 1.0;
const DEFAULT_F1_WHEN_NO_DATA: f64 = 0.0;

/// Ideal baseline scores for aggregate events — perfect precision and recall, zero noise.
const IDEAL_PRECISION: f64 = 1.0;
const IDEAL_RECALL: f64 = 1.0;
const IDEAL_NOISE_RATE: f64 = 0.0;

/// Report evaluation results to Braintrust monitoring dashboard.
///
/// Inserts events for each fixture and one aggregate row into a persistent
/// experiment named "snif-eval". Runs accumulate as data points within a
/// single experiment, enabling trend tracking without dashboard clutter.
///
/// Fails gracefully: returns an error but does not panic. The caller should
/// log the error and continue — local JSONL history is unaffected.
pub fn report_to_braintrust(
    api_key: &str,
    project_id: &str,
    model_name: &str,
    record: &EvalRecord,
    fixture_results: &[FixtureResult],
) -> Result<()> {
    let client = Client::builder()
        .timeout(std::time::Duration::from_secs(HTTP_TIMEOUT_SECS))
        .build()
        .context("Failed to create HTTP client")?;

    report_to_braintrust_inner(
        &client,
        BRAINTRUST_API_BASE,
        api_key,
        project_id,
        model_name,
        EXPERIMENT_NAME,
        record,
        fixture_results,
    )
}

/// Internal implementation, testable with a mock base URL.
fn report_to_braintrust_inner(
    client: &Client,
    api_base: &str,
    api_key: &str,
    project_id: &str,
    model_name: &str,
    experiment_name: &str,
    record: &EvalRecord,
    fixture_results: &[FixtureResult],
) -> Result<()> {
    // Step 1: Create the experiment
    let exp_id = create_experiment(
        client,
        api_base,
        api_key,
        project_id,
        model_name,
        experiment_name,
        record,
    )?;

    tracing::info!(
        experiment_id = %exp_id,
        name = EXPERIMENT_NAME,
        "Braintrust experiment created"
    );

    // Step 2: Insert per-fixture events
    insert_fixture_events(&client, api_base, api_key, &exp_id, model_name, fixture_results)?;

    // Step 3: Insert aggregate summary event
    insert_aggregate_event(&client, api_base, api_key, &exp_id, model_name, record)?;

    Ok(())
}

fn create_experiment(
    client: &Client,
    api_base: &str,
    api_key: &str,
    project_id: &str,
    model_name: &str,
    experiment_name: &str,
    record: &EvalRecord,
) -> Result<String> {
    let url = format!("{}/v1/experiment", api_base);

    let body = json!({
        "project_id": project_id,
        "name": experiment_name,
        "description": EXPERIMENT_DESCRIPTION,
        "repo_info": {
            "commit": record.git_sha,
        },
        "metadata": {
            "model": model_name,
            "timestamp": record.timestamp,
        },
        "ensure_new": false,
        "tags": [
            EVAL_TAG,
            if record.gates_passed { GATES_PASSED_TAG } else { GATES_FAILED_TAG },
        ],
    });

    let response = client
        .post(&url)
        .header("Authorization", format!("Bearer {}", api_key))
        .header("Content-Type", "application/json")
        .json(&body)
        .send()
        .context("Failed to send experiment creation request")?;

    if !response.status().is_success() {
        let status = response.status();
        let body_text = response.text().unwrap_or_default();
        anyhow::bail!("Braintrust API returned {}: {}", status, body_text);
    }

    let result: serde_json::Value = response
        .json()
        .context("Failed to parse experiment creation response")?;

    result
        .get("id")
        .and_then(|v| v.as_str())
        .map(String::from)
        .context("Experiment creation response missing 'id' field")
}

fn insert_fixture_events(
    client: &Client,
    api_base: &str,
    api_key: &str,
    experiment_id: &str,
    model_name: &str,
    fixture_results: &[FixtureResult],
) -> Result<()> {
    if fixture_results.is_empty() {
        return Ok(());
    }

    let url = format!(
        "{}/v1/experiment/{}/insert",
        api_base, experiment_id
    );

    let events: Vec<serde_json::Value> = fixture_results
        .iter()
        .enumerate()
        .map(|(i, fr)| {
            let fixture_precision = if fr.true_positives + fr.false_positives > 0 {
                fr.true_positives as f64 / (fr.true_positives + fr.false_positives) as f64
            } else {
                DEFAULT_SCORE_WHEN_NO_DATA
            };
            let fixture_recall = if fr.true_positives + fr.false_negatives > 0 {
                fr.true_positives as f64 / (fr.true_positives + fr.false_negatives) as f64
            } else {
                DEFAULT_SCORE_WHEN_NO_DATA
            };
            let fixture_f1 = if fixture_precision + fixture_recall > 0.0 {
                F1_COEFFICIENT * fixture_precision * fixture_recall / (fixture_precision + fixture_recall)
            } else {
                DEFAULT_F1_WHEN_NO_DATA
            };

            json!({
                "id": format!("fixture-{}-{}", i, fr.fixture_name),
                "input": {
                    "fixture": fr.fixture_name,
                    "expected_findings": fr.expected,
                },
                "output": {
                    "actual_findings": fr.actual,
                    "tp": fr.true_positives,
                    "fp": fr.false_positives,
                    "fn": fr.false_negatives,
                },
                "expected": {
                    "tp": fr.expected,
                    "fp": 0,
                    "fn": 0,
                },
                "scores": {
                    "precision": fixture_precision,
                    "recall": fixture_recall,
                    "f1": fixture_f1,
                },
                "metadata": {
                    "model": model_name,
                    "fixture_name": fr.fixture_name,
                },
            })
        })
        .collect();

    let body = json!({ "events": events });

    let response = client
        .post(&url)
        .header("Authorization", format!("Bearer {}", api_key))
        .header("Content-Type", "application/json")
        .json(&body)
        .send()
        .context("Failed to send fixture events request")?;

    if !response.status().is_success() {
        let status = response.status();
        let body_text = response.text().unwrap_or_default();
        tracing::warn!(
            status = %status,
            body = %body_text,
            "Braintrust insert returned non-success"
        );
        // Don't fail the whole report for this — event data is supplementary
    }

    tracing::debug!(count = fixture_results.len(), "Inserted fixture events");
    Ok(())
}

fn insert_aggregate_event(
    client: &Client,
    api_base: &str,
    api_key: &str,
    experiment_id: &str,
    model_name: &str,
    record: &EvalRecord,
) -> Result<()> {
    let url = format!(
        "{}/v1/experiment/{}/insert",
        api_base, experiment_id
    );

    let noise_score = 1.0 - record.noise_rate;

    let body = json!({
        "events": [{
            "id": "aggregate",
            "input": "overall-eval-summary",
            "output": {
                "precision": record.precision,
                "recall": record.recall,
                "noise_rate": record.noise_rate,
                "gates_passed": record.gates_passed,
                "fixture_count": record.fixture_count,
            },
            "expected": {
                "precision": IDEAL_PRECISION,
                "recall": IDEAL_RECALL,
                "noise_rate": IDEAL_NOISE_RATE,
            },
            "scores": {
                "precision": record.precision,
                "recall": record.recall,
                "noise-inverse": noise_score,
            },
            "metadata": {
                "model": model_name,
                "gates_passed": record.gates_passed,
                "fixture_count": record.fixture_count,
            },
        }],
    });

    let response = client
        .post(&url)
        .header("Authorization", format!("Bearer {}", api_key))
        .header("Content-Type", "application/json")
        .json(&body)
        .send()
        .context("Failed to send aggregate event request")?;

    if !response.status().is_success() {
        let status = response.status();
        let body_text = response.text().unwrap_or_default();
        tracing::warn!(
            status = %status,
            body = %body_text,
            "Braintrust aggregate insert returned non-success"
        );
    }

    tracing::debug!("Inserted aggregate event");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::metrics::FixtureResult;

    const TEST_API_KEY: &str = "test-api-key";
    const TEST_MODEL: &str = "test-model";

    fn test_record() -> EvalRecord {
        EvalRecord {
            timestamp: "2024-01-01T00:00:00Z".to_string(),
            git_sha: "abc1234def5678abc1234def5678abc1234def56".to_string(),
            fixture_count: 2,
            precision: 0.85,
            recall: 0.90,
            noise_rate: 0.05,
            gates_passed: true,
            per_fixture: vec![
                crate::history::FixtureRecord {
                    name: "basic-security".to_string(),
                    expected: 3,
                    actual: 3,
                    tp: 3,
                    fp: 0,
                    fn_count: 0,
                },
                crate::history::FixtureRecord {
                    name: "error-handling".to_string(),
                    expected: 2,
                    actual: 3,
                    tp: 2,
                    fp: 1,
                    fn_count: 0,
                },
            ],
        }
    }

    fn test_fixture_results() -> Vec<FixtureResult> {
        vec![
            FixtureResult {
                fixture_name: "basic-security".to_string(),
                expected: 3,
                actual: 3,
                true_positives: 3,
                false_positives: 0,
                false_negatives: 0,
            },
            FixtureResult {
                fixture_name: "error-handling".to_string(),
                expected: 2,
                actual: 3,
                true_positives: 2,
                false_positives: 1,
                false_negatives: 0,
            },
        ]
    }

    #[test]
    fn report_to_braintrust_full_flow() {
        let mut server = mockito::Server::new();

        server
            .mock("POST", "/v1/experiment")
            .with_status(200)
            .with_body(r#"{"id": "exp-123"}"#)
            .create();

        server
            .mock("POST", mockito::Matcher::Regex(r"^/v1/experiment/.+/insert$".to_string()))
            .with_status(200)
            .with_body(r#"{"row_ids": ["r1"]}"#)
            .expect_at_least(1)
            .create();

        let client = Client::builder()
            .timeout(std::time::Duration::from_secs(5))
            .build()
            .unwrap();

        let record = test_record();
        let fixture_results = test_fixture_results();

        let result = report_to_braintrust_inner(
            &client,
            &server.url(),
            TEST_API_KEY,
            BRAINTRUST_DEFAULT_PROJECT_ID,
            TEST_MODEL,
            EXPERIMENT_NAME,
            &record,
            &fixture_results,
        );

        assert!(result.is_ok());
    }

    #[test]
    fn insert_fixture_events_empty_results_succeeds() {
        // Empty fixture results should short-circuit insert_fixture_events,
        // but the full flow (experiment creation + aggregate insert) still runs.
        let mut server = mockito::Server::new();

        server
            .mock("POST", "/v1/experiment")
            .with_status(200)
            .with_body(r#"{"id": "exp-123"}"#)
            .create();

        server
            .mock("POST", mockito::Matcher::Regex(r"^/v1/experiment/.+/insert$".to_string()))
            .with_status(200)
            .with_body(r#"{"row_ids": ["r1"]}"#)
            .expect_at_least(1)
            .create();

        let client = Client::builder()
            .timeout(std::time::Duration::from_secs(5))
            .build()
            .unwrap();

        let record = test_record();
        let fixture_results: Vec<FixtureResult> = vec![];

        let result = report_to_braintrust_inner(
            &client,
            &server.url(),
            TEST_API_KEY,
            BRAINTRUST_DEFAULT_PROJECT_ID,
            TEST_MODEL,
            EXPERIMENT_NAME,
            &record,
            &fixture_results,
        );

        assert!(result.is_ok());
    }
}
