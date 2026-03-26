use anyhow::Result;
use snif_embeddings::Embedder;
use snif_types::Finding;

use crate::FeedbackStore;

#[derive(Debug, Clone)]
pub enum SignalType {
    Accepted,
    Dismissed,
    Ignored,
}

impl SignalType {
    pub fn as_str(&self) -> &str {
        match self {
            SignalType::Accepted => "accepted",
            SignalType::Dismissed => "dismissed",
            SignalType::Ignored => "ignored",
        }
    }
}

pub struct FeedbackSignal {
    pub finding: Finding,
    pub signal_type: SignalType,
}

pub fn collect_signals(
    store: &FeedbackStore,
    team_id: &str,
    signals: &[FeedbackSignal],
    embedder: &Embedder,
) -> Result<usize> {
    let mut collected = 0;

    for signal in signals {
        let finding_text = format!(
            "{}: {} — {}",
            signal.finding.location.file, signal.finding.explanation, signal.finding.impact
        );

        let embedding = embedder.embed_single(&finding_text)?;

        let signal_id = store.insert_signal(
            team_id,
            signal.signal_type.as_str(),
            &finding_text,
            &signal.finding.category.to_string(),
        )?;

        store.insert_signal_embedding(signal_id, &embedding)?;
        collected += 1;
    }

    tracing::info!(collected, team = team_id, "Feedback signals stored");
    Ok(collected)
}
