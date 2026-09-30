//! Validated, provider-neutral history imported from driftctl.
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
mod conflict_changes;
mod error;
mod events;
mod evidence_changes;
mod intent_changes;
mod items;
mod sources;
mod validation;
pub use error::*;
pub use events::*;
pub use items::*;
pub use sources::*;
use validation::*;

#[derive(Clone, Debug, Eq, PartialEq)]
struct ConflictState {
    conflict: Conflict,
    resolved: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct ReplayState {
    goal: Option<GoalRevision>,
    intents: BTreeMap<IntentId, IntentItem>,
    conflicts: BTreeMap<ConflictId, ConflictState>,
    evidence_invalidated: BTreeSet<EvidenceId>,
    closed: bool,
}

impl ReplayState {
    fn empty() -> Self {
        Self {
            goal: None,
            intents: BTreeMap::new(),
            conflicts: BTreeMap::new(),
            evidence_invalidated: BTreeSet::new(),
            closed: false,
        }
    }
}

/// Current replay state.  Inactive items are retained for auditability;
/// `open_conflicts` contains only unresolved conflicts.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IntentSnapshot {
    pub goal: GoalRevision,
    pub intents: BTreeMap<IntentId, IntentItem>,
    pub open_conflicts: BTreeMap<ConflictId, Conflict>,
    pub closed: bool,
}

/// An append-only v2 event history with deterministic replay.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct History {
    records: Vec<EventRecord>,
    state: ReplayState,
}

impl History {
    /// Start a new v2 history with its first `run_started` event.
    pub fn new(goal: GoalRevision) -> Result<Self, HistoryError> {
        let mut history = Self {
            records: Vec::new(),
            state: ReplayState::empty(),
        };
        history.append(Event::RunStarted { goal })?;
        Ok(history)
    }

    /// Replay an ordered sequence of versioned events without mutating it.
    pub fn replay<I>(records: I) -> Result<Self, HistoryError>
    where
        I: IntoIterator<Item = EventRecord>,
    {
        let mut history = Self {
            records: Vec::new(),
            state: ReplayState::empty(),
        };
        for record in records {
            history.apply_record(&record)?;
            history.records.push(record);
        }
        if history.records.is_empty() {
            return Err(HistoryError::EmptyHistory);
        }
        Ok(history)
    }

    /// Append one event after validating it against a cloned state.  A failed
    /// event leaves both the state and the ordered records untouched.
    pub fn append(&mut self, event: Event) -> Result<EventRecord, HistoryError> {
        let sequence = u64::try_from(self.records.len())
            .ok()
            .and_then(|length| length.checked_add(1))
            .ok_or_else(|| HistoryError::InvalidTransition {
                event: "append",
                reason: "event sequence exhausted".to_owned(),
            })?;
        let record = EventRecord::new(sequence, event);
        self.apply_record(&record)?;
        self.records.push(record.clone());
        Ok(record)
    }

    /// The immutable ordered records, suitable for persistence by a caller.
    #[must_use]
    pub fn records(&self) -> &[EventRecord] {
        &self.records
    }

    /// Alias emphasizing that records are the event stream.
    #[must_use]
    pub fn events(&self) -> &[EventRecord] {
        self.records()
    }

    #[must_use]
    pub fn goal(&self) -> &GoalRevision {
        self.state
            .goal
            .as_ref()
            .expect("a valid history always has a goal")
    }

    #[must_use]
    pub fn intent(&self, id: impl AsRef<str>) -> Option<&IntentItem> {
        self.state
            .intents
            .iter()
            .find(|(intent_id, _)| intent_id.as_str() == id.as_ref())
            .map(|(_, item)| item)
    }

    #[must_use]
    pub fn intents(&self) -> &BTreeMap<IntentId, IntentItem> {
        &self.state.intents
    }

    #[must_use]
    pub fn evidence_is_valid(&self, id: &EvidenceId) -> bool {
        self.state
            .intents
            .values()
            .any(|item| item.evidence.iter().any(|evidence| &evidence.id == id))
            && !self.state.evidence_invalidated.contains(id)
    }

    #[must_use]
    pub fn open_conflicts(&self) -> BTreeMap<ConflictId, Conflict> {
        self.state
            .conflicts
            .iter()
            .filter(|(_, conflict)| !conflict.resolved)
            .map(|(id, conflict)| (id.clone(), conflict.conflict.clone()))
            .collect()
    }

    #[must_use]
    pub fn is_closed(&self) -> bool {
        self.state.closed
    }

    #[must_use]
    pub fn snapshot(&self) -> IntentSnapshot {
        IntentSnapshot {
            goal: self.goal().clone(),
            intents: self.state.intents.clone(),
            open_conflicts: self.open_conflicts(),
            closed: self.state.closed,
        }
    }

    /// Serialize the ordered event stream as a JSON array.
    pub fn to_json(&self) -> Result<String, HistoryError> {
        serde_json::to_string(&self.records).map_err(HistoryError::from)
    }

    /// Deserialize and replay a JSON array of ordered event records.
    pub fn from_json(input: &str) -> Result<Self, HistoryError> {
        let records: Vec<EventRecord> = serde_json::from_str(input)?;
        Self::replay(records)
    }

    /// Serialize the ordered event stream as JSONL for append-oriented stores.
    pub fn to_jsonl(&self) -> Result<String, HistoryError> {
        self.records
            .iter()
            .map(serde_json::to_string)
            .collect::<Result<Vec<_>, _>>()
            .map(|lines| format!("{}\n", lines.join("\n")))
            .map_err(HistoryError::from)
    }

    /// Deserialize and replay JSONL records.  A final empty line is accepted
    /// as the normal JSONL trailing newline, while nonempty lines remain
    /// strictly ordered and validated.
    pub fn from_jsonl(input: &str) -> Result<Self, HistoryError> {
        let mut records = Vec::new();
        for (line_number, line) in input.lines().enumerate() {
            if line.trim().is_empty() {
                continue;
            }
            let record = serde_json::from_str(line).map_err(|error| {
                HistoryError::Serialization(format!(
                    "invalid intent history line {}: {error}",
                    line_number + 1
                ))
            })?;
            records.push(record);
        }
        Self::replay(records)
    }

    fn apply_record(&mut self, record: &EventRecord) -> Result<(), HistoryError> {
        let expected = u64::try_from(self.records.len())
            .ok()
            .and_then(|length| length.checked_add(1))
            .ok_or_else(|| HistoryError::InvalidTransition {
                event: "replay",
                reason: "event sequence exhausted".to_owned(),
            })?;
        if record.schema_version != HISTORY_SCHEMA_VERSION {
            return Err(HistoryError::UnsupportedSchemaVersion {
                expected: HISTORY_SCHEMA_VERSION,
                found: record.schema_version,
            });
        }
        if record.sequence != expected {
            return Err(HistoryError::SequenceGap {
                expected,
                found: record.sequence,
            });
        }

        let mut next = self.state.clone();
        apply_event(&mut next, &record.event)?;
        self.state = next;
        Ok(())
    }
}

fn apply_event(state: &mut ReplayState, event: &Event) -> Result<(), HistoryError> {
    if state.closed {
        return Err(HistoryError::EventAfterClosure);
    }
    match event {
        Event::RunStarted { goal } => intent_changes::run_started(state, goal),
        Event::RequirementAdded { item, approval } => {
            intent_changes::requirement_added(state, item, approval)
        }
        Event::RequirementSuperseded {
            previous_id,
            replacement,
            approval,
        } => intent_changes::requirement_superseded(state, previous_id, replacement, approval),
        Event::RequirementWithdrawn {
            intent_id,
            source_refs,
            approval,
        } => intent_changes::requirement_withdrawn(state, intent_id, source_refs, approval),
        Event::RequirementConfirmed {
            intent_id,
            source_refs,
        } => intent_changes::requirement_confirmed(state, intent_id, source_refs),
        Event::ConflictRaised { conflict, approval } => {
            conflict_changes::conflict_raised(state, conflict, approval)
        }
        Event::ConflictResolved {
            conflict_id,
            resolution,
        } => conflict_changes::conflict_resolved(state, conflict_id, resolution),
        Event::GoalRevised { goal } => intent_changes::goal_revised(state, goal),
        Event::EvidenceAttached {
            intent_id,
            evidence,
        } => evidence_changes::evidence_attached(state, intent_id, evidence),
        Event::EvidenceInvalidated {
            intent_id,
            evidence_id,
            source_refs,
            reason,
        } => evidence_changes::evidence_invalidated(
            state,
            intent_id,
            evidence_id,
            source_refs,
            reason,
        ),
        Event::RunClosed {
            source_refs,
            approval,
        } => intent_changes::run_closed(state, source_refs, approval),
    }
}
// Convenient names used by callers that prefer the architecture vocabulary.
pub type HistoryRecord = EventRecord;
pub type VersionedEvent = EventRecord;
pub type IntentEvent = Event;
pub type Lifecycle = IntentLifecycle;
