use anyhow::Result;
use snif_config::FilterConfig;
use snif_embeddings::Embedder;
use snif_types::Finding;

use crate::FeedbackStore;

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
            if *distance > 0.3 {
                continue;
            }
            match signal_type.as_str() {
                "dismissed" => dismissed_count += 1,
                "accepted" => accepted_count += 1,
                _ => {}
            }
        }

        if dismissed_count >= 3 {
            tracing::debug!(
                file = %finding.location.file,
                dismissed = dismissed_count,
                "Feedback filter: suppressed (similar to dismissed findings)"
            );
            continue;
        }

        if accepted_count >= 3 {
            finding.confidence = (finding.confidence + 0.1).min(1.0);
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
