use anyhow::Result;
use snif_config::FilterConfig;
use snif_embeddings::Embedder;
use snif_types::Finding;

use crate::collector::SignalType;
use crate::FeedbackStore;

/// Maximum cosine distance for a signal to be considered similar.
const MAX_SIMILAR_DISTANCE: f64 = 0.3;

/// Minimum number of dismissed signals to suppress a finding.
const DISMISSED_SUPPRESSION_THRESHOLD: usize = 3;

/// Minimum number of accepted signals to boost finding confidence.
const ACCEPTED_BOOST_THRESHOLD: usize = 3;

/// Confidence boost applied when a finding matches accepted signals.
const ACCEPTED_CONFIDENCE_BOOST: f64 = 0.1;

pub fn apply_feedback_filter(
    findings: Vec<Finding>,
    store: &FeedbackStore,
    team_id: &str,
    embedder: &Embedder,
    config: &FilterConfig,
) -> Result<Vec<Finding>> {
    let signal_count = store.get_signal_count(team_id)?;

    if signal_count < config.feedback_min_signals {
        tracing::debug!(
            signals = signal_count,
            threshold = config.feedback_min_signals,
            "Feedback filter inactive — not enough signals"
        );
        return Ok(findings);
    }

    let before = findings.len();
    let mut filtered = Vec::with_capacity(findings.len());

    for mut finding in findings {
        let finding_text = format!(
            "{}: {} — {}",
            finding.location.file, finding.explanation, finding.impact
        );

        let embedding = match embedder.embed_single(&finding_text) {
            Ok(e) => e,
            Err(_) => {
                filtered.push(finding);
                continue;
            }
        };

        let similar = store.query_similar_signals(&embedding, team_id, 10)?;

        let mut dismissed_count = 0;
        let mut accepted_count = 0;

        for (signal_type, distance) in &similar {
            if *distance > MAX_SIMILAR_DISTANCE {
                continue;
            }
            match signal_type.as_str() {
                s if s == SignalType::Dismissed.as_str() => dismissed_count += 1,
                s if s == SignalType::Accepted.as_str() => accepted_count += 1,
                _ => {}
            }
        }

        if dismissed_count >= DISMISSED_SUPPRESSION_THRESHOLD {
            tracing::debug!(
                file = %finding.location.file,
                dismissed = dismissed_count,
                "Feedback filter: suppressed (similar to dismissed findings)"
            );
            continue;
        }

        if accepted_count >= ACCEPTED_BOOST_THRESHOLD {
            finding.confidence = (finding.confidence + ACCEPTED_CONFIDENCE_BOOST).min(1.0);
        }

        if finding.confidence >= config.min_confidence {
            filtered.push(finding);
        }
    }

    let after = filtered.len();
    tracing::info!(
        before,
        after,
        suppressed = before - after,
        "Feedback filter applied"
    );

    Ok(filtered)
}
