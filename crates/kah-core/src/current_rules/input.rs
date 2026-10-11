use super::*;
use crate::intent_history::{Event, History};
use serde_json::{Value, json};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Source {
    pub id: String,
    pub text: String,
    pub content_digest: String,
    /// Tagged separately: history events are not standing-file user messages.
    pub origin: Value,
}
impl Source {
    pub fn standing(path: &str, text: &str, provenance: Value) -> Self {
        Self::new(
            format!("standing:{path}"),
            text.into(),
            json!({
                "kind":"standing_file", "path":path, "provenance":provenance
            }),
        )
    }
    fn new(id: String, text: String, origin: Value) -> Self {
        let content_digest = format!("sha256:{:x}", Sha256::digest(text.as_bytes()));
        Self {
            id,
            text,
            content_digest,
            origin,
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Input {
    pub history_digest: String,
    pub revision: u64,
    /// Informational clock, not a licence to retire a rule by age.
    pub observed_at: String,
    pub sources: Vec<Source>,
    pub history: Vec<crate::intent_history::EventRecord>,
}
impl Input {
    pub fn from_history(
        history: &History,
        standing: Vec<Source>,
        observed_at: &str,
    ) -> Result<Self, String> {
        if standing.iter().any(|s| s.origin["kind"] != "standing_file") {
            return Err(error("SOURCE", "standing inputs must have file provenance"));
        }
        let mut sources = history_sources(history)?;
        sources.extend(standing);
        Ok(Self {
            history_digest: digest(history.records())?,
            revision: history.records().last().map_or(0, |r| r.sequence),
            observed_at: observed_at.into(),
            sources,
            history: history.records().to_vec(),
        })
    }
    /// Clock changes alone do not invalidate the projection.
    pub fn digest(&self) -> Result<String, String> {
        digest(&(&self.history_digest, self.revision, &self.sources))
    }
    pub(super) fn validate(&self) -> Result<(), String> {
        let history =
            History::replay(self.history.clone()).map_err(|e| error("HISTORY", &e.to_string()))?;
        let standing = self
            .sources
            .iter()
            .filter(|s| s.origin["kind"] == "standing_file")
            .cloned()
            .collect();
        let reconstructed = Self::from_history(&history, standing, &self.observed_at)?;
        if reconstructed != *self {
            return Err(error("SOURCE", "input differs from History replay"));
        }
        let mut ids = std::collections::BTreeSet::new();
        for source in &self.sources {
            if source.id.is_empty()
                || !ids.insert(&source.id)
                || (source.text.is_empty() && source.origin["kind"] != "standing_file")
                || source.content_digest
                    != format!("sha256:{:x}", Sha256::digest(source.text.as_bytes()))
            {
                return Err(error("SOURCE", "duplicate, empty or changed source"));
            }
        }
        Ok(())
    }
}

fn history_sources(history: &History) -> Result<Vec<Source>, String> {
    let mut sources = Vec::new();
    for record in history.records() {
        let mut origin = json!({"kind":"history", "sequence":record.sequence, "event":record});
        match &record.event {
            Event::RunStarted { goal } | Event::GoalRevised { goal } => {
                sources.push(Source::new(
                    format!("goal:{}", record.sequence),
                    goal.text.clone(),
                    origin,
                ));
            }
            Event::RequirementAdded { item, .. }
            | Event::RequirementSuperseded {
                replacement: item, ..
            } => {
                origin["current_lifecycle"] =
                    serde_json::to_value(&history.intents()[&item.id].lifecycle)
                        .map_err(|e| error("JSON", &e.to_string()))?;
                sources.push(Source::new(
                    format!("intent:{}", item.id),
                    item.text.clone(),
                    origin,
                ));
            }
            Event::EvidenceAttached { evidence, .. } => {
                sources.push(Source::new(
                    format!("evidence:{}", record.sequence),
                    evidence.summary.clone(),
                    origin,
                ));
            }
            Event::EvidenceInvalidated { reason, .. } => {
                sources.push(Source::new(
                    format!("invalidation:{}", record.sequence),
                    reason.clone(),
                    origin,
                ));
            }
            _ => {}
        }
    }
    Ok(sources)
}
