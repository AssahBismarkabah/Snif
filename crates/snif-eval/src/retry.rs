use snif_types::Finding;

#[derive(Debug, Clone)]
struct FindingGroup {
    representative: Finding,
    run_count: usize,
    last_seen_run: Option<usize>,
}

pub fn aggregate_findings(
    all_runs: &[Vec<Finding>],
    threshold: usize,
    line_tolerance: usize,
) -> Vec<Finding> {
    let threshold = threshold.max(1);
    let mut groups: Vec<FindingGroup> = Vec::new();

    for (run_index, findings) in all_runs.iter().enumerate() {
        for finding in findings {
            if let Some(group) = groups
                .iter_mut()
                .find(|group| findings_match(&group.representative, finding, line_tolerance))
            {
                if group.last_seen_run != Some(run_index) {
                    group.run_count += 1;
                    group.last_seen_run = Some(run_index);
                }
                continue;
            }

            groups.push(FindingGroup {
                representative: finding.clone(),
                run_count: 1,
                last_seen_run: Some(run_index),
            });
        }
    }

    let accepted: Vec<Finding> = groups
        .iter()
        .filter(|group| group.run_count >= threshold)
        .map(|group| group.representative.clone())
        .collect();

    tracing::info!(
        runs = all_runs.len(),
        candidates = groups.len(),
        accepted = accepted.len(),
        threshold,
        "Aggregated retry findings"
    );

    accepted
}

fn findings_match(left: &Finding, right: &Finding, line_tolerance: usize) -> bool {
    let line_diff = (left.location.start_line as i64 - right.location.start_line as i64)
        .unsigned_abs() as usize;

    left.location.file == right.location.file
        && left.category == right.category
        && line_diff <= line_tolerance
}

#[cfg(test)]
mod tests {
    use super::*;
    use snif_types::{FileLocation, FindingCategory};

    const TEST_FILE: &str = "src/parser.ts";
    const OTHER_FILE: &str = "src/other.ts";
    const TEST_CONFIDENCE: f64 = 0.9;
    const TEST_TOLERANCE: usize = 5;

    fn make_finding(file: &str, line: usize, category: FindingCategory) -> Finding {
        Finding {
            location: FileLocation {
                file: file.to_string(),
                start_line: line,
                end_line: None,
            },
            category,
            confidence: TEST_CONFIDENCE,
            evidence: "evidence".to_string(),
            explanation: "explanation".to_string(),
            impact: "impact".to_string(),
            suggestion: None,
            fingerprint: None,
        }
    }

    #[test]
    fn aggregate_accepts_majority_finding_with_line_tolerance() {
        let runs = vec![
            vec![make_finding(TEST_FILE, 6, FindingCategory::Security)],
            vec![],
            vec![make_finding(TEST_FILE, 9, FindingCategory::Security)],
        ];

        let aggregated = aggregate_findings(&runs, 2, TEST_TOLERANCE);

        assert_eq!(aggregated.len(), 1);
        assert_eq!(aggregated[0].location.start_line, 6);
    }

    #[test]
    fn aggregate_rejects_single_run_finding_when_majority_required() {
        let runs = vec![
            vec![make_finding(TEST_FILE, 6, FindingCategory::Security)],
            vec![],
            vec![],
        ];

        let aggregated = aggregate_findings(&runs, 2, TEST_TOLERANCE);

        assert!(aggregated.is_empty());
    }

    #[test]
    fn aggregate_counts_duplicate_findings_once_per_run() {
        let runs = vec![
            vec![
                make_finding(TEST_FILE, 6, FindingCategory::Security),
                make_finding(TEST_FILE, 7, FindingCategory::Security),
            ],
            vec![],
            vec![],
        ];

        let aggregated = aggregate_findings(&runs, 2, TEST_TOLERANCE);

        assert!(aggregated.is_empty());
    }

    #[test]
    fn aggregate_keeps_file_and_category_separate() {
        let runs = vec![
            vec![make_finding(TEST_FILE, 6, FindingCategory::Security)],
            vec![make_finding(OTHER_FILE, 6, FindingCategory::Security)],
            vec![make_finding(TEST_FILE, 6, FindingCategory::Logic)],
        ];

        let aggregated = aggregate_findings(&runs, 2, TEST_TOLERANCE);

        assert!(aggregated.is_empty());
    }
}
