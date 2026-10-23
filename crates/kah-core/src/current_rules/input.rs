use super::*;
use crate::intent_history::{Event, History, IntentId};
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
    pub fn intake(
        record: &str,
        text: &str,
        source: &crate::intent_history::SourceRef,
        non_authority_sender: bool,
    ) -> Self {
        Self::new(
            format!("intake:{record}"),
            text.into(),
            json!({"kind":"intake_verification","record":record,"source":source,"non_authority_sender":non_authority_sender}),
        )
    }
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
        if standing.iter().any(|s| {
            !matches!(
                s.origin["kind"].as_str(),
                Some("standing_file" | "intake_verification")
            )
        }) {
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
    /// The source of an item's current words: its latest resolution source
    /// that still matches, else its recorded `intent:` words when unchanged.
    pub(super) fn current_words(&self, history: &History, id: &IntentId) -> Option<&str> {
        let item = history.intents().get(id)?;
        self.sources
            .iter()
            .rev()
            .find(|s| {
                s.text == item.text
                    && (s.id == format!("intent:{id}")
                        || s.id.starts_with("resolution:") && s.origin["intent_id"] == id.as_str())
            })
            .map(|s| s.id.as_str())
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
            .filter(|s| {
                matches!(
                    s.origin["kind"].as_str(),
                    Some("standing_file" | "intake_verification")
                )
            })
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
    let mut raised = std::collections::BTreeMap::new();
    for record in history.records() {
        let origin = json!({"kind":"history", "sequence":record.sequence, "event":record});
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
            } => sources.push(intent_source(history, item, origin)?),
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
            Event::ConflictRaised { conflict, .. } => {
                raised.insert(conflict.id.clone(), conflict.clone());
            }
            Event::ConflictResolved {
                conflict_id,
                resolution,
            } => {
                let conflict = raised
                    .get(conflict_id)
                    .ok_or_else(|| error("HISTORY", "resolution of an unknown conflict"))?;
                sources.extend(resolution_sources(
                    history, record, conflict, resolution, &origin,
                )?);
            }
            _ => {}
        }
    }
    Ok(sources)
}

/// An item's words as recorded, labelled with its current lifecycle. A
/// conflict resolution may have replaced these words since.
fn intent_source(
    history: &History,
    item: &crate::intent_history::IntentItem,
    mut origin: Value,
) -> Result<Source, String> {
    let current = &history.intents()[&item.id];
    origin["current_lifecycle"] = if current.text == item.text {
        serde_json::to_value(&current.lifecycle).map_err(|e| error("JSON", &e.to_string()))?
    } else {
        json!(REPLACED_BY_RESOLUTION)
    };
    Ok(Source::new(
        format!("intent:{}", item.id),
        item.text.clone(),
        origin,
    ))
}

/// Lifecycle label of history words a conflict resolution has replaced.
const REPLACED_BY_RESOLUTION: &str = "replaced_by_resolution";

/// The chosen alternative's words, one source per affected item so each
/// keeps its own association. A source carries the item's current lifecycle
/// while the item still has these words.
fn resolution_sources(
    history: &History,
    record: &crate::intent_history::EventRecord,
    conflict: &crate::intent_history::Conflict,
    resolution: &crate::intent_history::ConflictResolution,
    origin: &Value,
) -> Result<Vec<Source>, String> {
    let text = conflict
        .alternatives
        .iter()
        .find(|alternative| alternative.id == resolution.alternative_id)
        .map(|alternative| alternative.text.clone())
        .ok_or_else(|| error("HISTORY", "resolution selects no alternative"))?;
    let mut sources = Vec::new();
    for id in &conflict.intent_ids {
        let item = history
            .intents()
            .get(id)
            .ok_or_else(|| error("HISTORY", "resolution of an unknown item"))?;
        let mut origin = origin.clone();
        origin["intent_id"] = json!(id);
        origin["current_lifecycle"] = if item.text == text {
            serde_json::to_value(&item.lifecycle).map_err(|e| error("JSON", &e.to_string()))?
        } else {
            json!(REPLACED_BY_RESOLUTION)
        };
        sources.push(Source::new(
            format!("resolution:{}:{id}", record.sequence),
            text.clone(),
            origin,
        ));
    }
    Ok(sources)
}
