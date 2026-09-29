use super::*;

/// A domain event.  [`EventRecord`] supplies its immutable schema and
/// sequence envelope.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Event {
    RunStarted {
        goal: GoalRevision,
    },
    RequirementAdded {
        item: IntentItem,
        approval: Option<Approval>,
    },
    RequirementSuperseded {
        previous_id: IntentId,
        replacement: IntentItem,
        approval: Option<Approval>,
    },
    RequirementWithdrawn {
        intent_id: IntentId,
        source_refs: Vec<SourceRef>,
        approval: Option<Approval>,
    },
    ConflictRaised {
        conflict: Conflict,
        approval: Option<Approval>,
    },
    ConflictResolved {
        conflict_id: ConflictId,
        resolution: ConflictResolution,
    },
    GoalRevised {
        goal: GoalRevision,
    },
    EvidenceAttached {
        intent_id: IntentId,
        evidence: EvidenceRef,
    },
    EvidenceInvalidated {
        intent_id: IntentId,
        evidence_id: EvidenceId,
        source_refs: Vec<SourceRef>,
        reason: String,
    },
    RunClosed {
        source_refs: Vec<SourceRef>,
        approval: Option<Approval>,
    },
}

/// An immutable event with an exact schema version and contiguous sequence.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct EventRecord {
    pub schema_version: u32,
    pub sequence: u64,
    #[serde(flatten)]
    pub event: Event,
}

impl EventRecord {
    #[must_use]
    pub fn new(sequence: u64, event: Event) -> Self {
        Self {
            schema_version: HISTORY_SCHEMA_VERSION,
            sequence,
            event,
        }
    }
}
