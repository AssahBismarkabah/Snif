use crate::history::EvalRecord;

/// Guidance text generated from analysis of past eval runs.
/// Appended to the system prompt to steer the model based on
/// observed precision, recall, and noise trends.
#[derive(Debug, Clone, Default)]
pub struct EvalGuidance {
    pub prompt_augmentation: String,
}

/// Analyzes the last N eval records and produces guidance text
/// that can be injected into the system prompt before the next run.
///
/// Strategy:
/// - Precision declining → tell the model to be more conservative
/// - Recall declining → tell the model to be more thorough on missed areas
/// - Noise rising → suppress weak categories
/// - Persistent per-fixture FP/FN patterns → targeted instructions
pub fn analyze_history(history: &[EvalRecord], window: usize) -> EvalGuidance {
    if history.is_empty() {
        return EvalGuidance::default();
    }

    let recent: Vec<&EvalRecord> = history.iter().rev().take(window).collect();
    if recent.len() < 2 {
        // Not enough data for trend analysis; still check for persistent fixture patterns
        return analyze_fixture_patterns(&recent);
    }

    let mut lines: Vec<String> = Vec::new();

    // Trend analysis
    let precision_trend = compute_trend(&recent, |r| r.precision);
    let recall_trend = compute_trend(&recent, |r| r.recall);
    let noise_trend = compute_trend(&recent, |r| r.noise_rate);

    lines.push(String::from(
        "## Recent Evaluation Feedback\n\n\
         Based on analysis of recent evaluation runs, adjust your review approach:",
    ));

    if precision_trend < -0.05 {
        lines.push(String::from(
            "- Precision has declined recently. Be more conservative — only report \
             findings with clear, concrete evidence and user-visible impact. \
             When in doubt, stay quiet.",
        ));
    } else if precision_trend > 0.02 {
        lines.push(String::from(
            "- Precision is strong and trending up. Maintain this level of rigor.",
        ));
    }

    if recall_trend < -0.05 {
        lines.push(String::from(
            "- Recall has declined — findings are being missed. Be more thorough, \
             especially around error handling, resource management, and edge cases.",
        ));
    } else if recall_trend > 0.02 {
        lines.push(String::from(
            "- Recall is strong and trending up.",
        ));
    }

    if noise_trend > 0.05 {
        lines.push(String::from(
            "- Noise rate (false positives) is rising. Avoid flagging speculative issues, \
             code style, or patterns that don't have a clear behavioral impact.",
        ));
    }

    // Fixture-level pattern analysis
    let fixture_guidance = analyze_fixture_patterns(&recent);
    if !fixture_guidance.prompt_augmentation.is_empty() {
        lines.push(fixture_guidance.prompt_augmentation);
    }

    let mut guidance = EvalGuidance::default();
    if lines.len() > 1 {
        // More than just the header
        guidance.prompt_augmentation = lines.join("\n");
    }

    guidance
}

fn analyze_fixture_patterns(recent: &[&EvalRecord]) -> EvalGuidance {
    if recent.is_empty() {
        return EvalGuidance::default();
    }

    // Aggregate per-fixture stats across recent runs
    let mut fixture_fp_counts: std::collections::HashMap<String, usize> =
        std::collections::HashMap::new();
    let mut fixture_fn_counts: std::collections::HashMap<String, usize> =
        std::collections::HashMap::new();
    let mut fixture_runs: std::collections::HashMap<String, usize> =
        std::collections::HashMap::new();

    for record in recent {
        for fix in &record.per_fixture {
            let runs = fixture_runs.entry(fix.name.clone()).or_insert(0);
            *runs += 1;
            if fix.fp > 0 {
                let count = fixture_fp_counts.entry(fix.name.clone()).or_insert(0);
                *count += fix.fp;
            }
            if fix.fn_count > 0 {
                let count = fixture_fn_counts.entry(fix.name.clone()).or_insert(0);
                *count += fix.fn_count;
            }
        }
    }

    let mut lines: Vec<String> = Vec::new();

    // Find fixtures with persistent FP issues (FP in > 50% of runs)
    let persistent_fp: Vec<(&String, &usize)> = fixture_fp_counts
        .iter()
        .filter(|(name, fp_count)| {
            let runs = fixture_runs.get(name.as_str()).copied().unwrap_or(1);
            runs >= 2 && **fp_count as f64 / runs as f64 > 0.5
        })
        .collect();

    if !persistent_fp.is_empty() {
        let names: Vec<&str> = persistent_fp.iter().map(|(n, _)| n.as_str()).collect();
        lines.push(format!(
            "- The following fixtures have produced persistent false positives: {}. \
             These are likely clean or stylistic changes. Only report findings if you \
             identify a clear bug with concrete evidence.",
            names.join(", ")
        ));
    }

    // Find fixtures with persistent FN issues (missed in > 50% of runs)
    let persistent_fn: Vec<(&String, &usize)> = fixture_fn_counts
        .iter()
        .filter(|(name, fn_count)| {
            let runs = fixture_runs.get(name.as_str()).copied().unwrap_or(1);
            runs >= 2 && **fn_count as f64 / runs as f64 > 0.5
        })
        .collect();

    if !persistent_fn.is_empty() {
        let names: Vec<&str> = persistent_fn.iter().map(|(n, _)| n.as_str()).collect();
        lines.push(format!(
            "- The following fixtures have had findings missed in recent runs: {}. \
             Pay close attention to these patterns — they contain real bugs.",
            names.join(", ")
        ));
    }

    let mut guidance = EvalGuidance::default();
    if !lines.is_empty() {
        guidance.prompt_augmentation = lines.join("\n");
    }
    guidance
}

fn compute_trend(records: &[&EvalRecord], metric: fn(&EvalRecord) -> f64) -> f64 {
    if records.len() < 2 {
        return 0.0;
    }
    // `records` comes from `history.iter().rev().take(window)`,
    // so records[0] is the most recent, records[last] is the oldest.
    let newest = metric(records[0]);
    let oldest = metric(records.last().unwrap());
    newest - oldest
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::history::FixtureRecord;

    fn make_record(
        precision: f64,
        recall: f64,
        noise: f64,
        fixtures: Vec<(&str, usize, usize, usize, usize)>,
    ) -> EvalRecord {
        EvalRecord {
            timestamp: "2026-04-10T00:00:00Z".to_string(),
            git_sha: "test".to_string(),
            fixture_count: fixtures.len(),
            precision,
            recall,
            noise_rate: noise,
            gates_passed: true,
            per_fixture: fixtures
                .into_iter()
                .map(|(name, expected, actual, tp, fp)| FixtureRecord {
                    name: name.to_string(),
                    expected,
                    actual,
                    tp,
                    fp,
                    fn_count: if tp == 0 && expected > 0 { 1 } else { 0 },
                })
                .collect(),
        }
    }

    #[test]
    fn empty_history_returns_empty_guidance() {
        let guidance = analyze_history(&[], 5);
        assert!(guidance.prompt_augmentation.is_empty());
    }

    #[test]
    fn single_record_returns_no_trend_guidance() {
        let history = vec![make_record(0.80, 0.90, 0.10, vec![])];
        let guidance = analyze_history(&history, 5);
        // Single record, no trend possible; fixture patterns need 2+ runs
        assert!(guidance.prompt_augmentation.is_empty());
    }

    #[test]
    fn declining_precision_generates_conservative_guidance() {
        let history = vec![
            make_record(0.95, 0.90, 0.05, vec![]),
            make_record(0.90, 0.90, 0.10, vec![]),
            make_record(0.84, 0.96, 0.16, vec![]),
        ];
        let guidance = analyze_history(&history, 5);
        assert!(
            guidance
                .prompt_augmentation
                .contains("more conservative"),
            "expected conservative guidance, got: {}",
            guidance.prompt_augmentation
        );
    }

    #[test]
    fn rising_noise_generates_suppression_guidance() {
        let history = vec![
            make_record(0.95, 0.90, 0.04, vec![]),
            make_record(0.90, 0.90, 0.10, vec![]),
            make_record(0.84, 0.96, 0.16, vec![]),
        ];
        let guidance = analyze_history(&history, 5);
        assert!(
            guidance.prompt_augmentation.contains("Noise rate")
                || guidance.prompt_augmentation.contains("false positive"),
            "expected noise guidance, got: {}",
            guidance.prompt_augmentation
        );
    }

    #[test]
    fn persistent_fp_fixtures_generates_targeted_guidance() {
        let history = vec![
            make_record(
                0.90,
                0.90,
                0.10,
                vec![("style-ts", 5, 7, 5, 2)], // FP present
            ),
            make_record(
                0.85,
                0.90,
                0.15,
                vec![("style-ts", 5, 8, 5, 3)], // FP again
            ),
        ];
        let guidance = analyze_history(&history, 5);
        assert!(
            guidance
                .prompt_augmentation
                .contains("persistent false positive")
                || guidance
                    .prompt_augmentation
                    .contains("clean or stylistic"),
            "expected persistent-fixture guidance, got: {}",
            guidance.prompt_augmentation
        );
    }
}
