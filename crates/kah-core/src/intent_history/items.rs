use super::*;

/// The semantic category of an intent item.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum IntentKind {
    Outcome,
    Constraint,
    Invariant,
    Scope,
    Validation,
    StopCondition,
}

/// Lifecycle is intentionally independent of [`EvidenceState`].
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum IntentLifecycle {
    Active,
    Superseded,
    Withdrawn,
    Conflicted,
}

/// Evidence state is intentionally independent of [`IntentLifecycle`].
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceState {
    Unresolved,
    Satisfied,
    Reopened,
}

/// Evidence attached to an intent item.  Its source may be assistant/tool
/// output because evidence is not itself semantic intent authority.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct EvidenceRef {
    pub id: EvidenceId,
    pub summary: String,
    pub source_refs: Vec<SourceRef>,
}

impl EvidenceRef {
    #[must_use]
    pub fn new(
        id: impl Into<EvidenceId>,
        summary: impl Into<String>,
        source_refs: Vec<SourceRef>,
    ) -> Self {
        Self {
            id: id.into(),
            summary: summary.into(),
            source_refs,
        }
    }
}

/// A source-linked intent item retained in history even after it is inactive.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct IntentItem {
    pub id: IntentId,
    pub kind: IntentKind,
    pub text: String,
    pub lifecycle: IntentLifecycle,
    pub evidence_state: EvidenceState,
    pub introduced_by: Vec<SourceRef>,
    pub changed_by: Vec<SourceRef>,
    pub supersedes: Vec<IntentId>,
    pub evidence: Vec<EvidenceRef>,
    /// Uncertain replacement targets; both sides remain active for review.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub uncertain_supersedes: Vec<IntentId>,
    /// Retained user text whose steering presence is unconfirmed.
    #[serde(default, skip_serializing_if = "is_false")]
    pub review_required: bool,
    /// Category is uncertain; this alone never blocks recording.
    #[serde(default, skip_serializing_if = "is_false")]
    pub kind_uncertain: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub user_confirmed: bool,
}

impl IntentItem {
    #[must_use]
    pub fn new(
        id: impl Into<IntentId>,
        kind: IntentKind,
        text: impl Into<String>,
        introduced_by: Vec<SourceRef>,
    ) -> Self {
        Self {
            id: id.into(),
            kind,
            text: text.into(),
            lifecycle: IntentLifecycle::Active,
            evidence_state: EvidenceState::Unresolved,
            introduced_by,
            changed_by: Vec::new(),
            supersedes: Vec::new(),
            evidence: Vec::new(),
            uncertain_supersedes: Vec::new(),
            review_required: false,
            kind_uncertain: false,
            user_confirmed: false,
        }
    }

    /// Construct the replacement side of a supersession edge.
    #[must_use]
    pub fn superseding(
        id: impl Into<IntentId>,
        kind: IntentKind,
        text: impl Into<String>,
        introduced_by: Vec<SourceRef>,
        previous_id: impl Into<IntentId>,
    ) -> Self {
        Self {
            id: id.into(),
            kind,
            text: text.into(),
            lifecycle: IntentLifecycle::Active,
            evidence_state: EvidenceState::Unresolved,
            changed_by: introduced_by.clone(),
            introduced_by,
            supersedes: vec![previous_id.into()],
            evidence: Vec::new(),
            uncertain_supersedes: Vec::new(),
            review_required: false,
            kind_uncertain: false,
            user_confirmed: false,
        }
    }

    #[must_use]
    pub fn is_active(&self) -> bool {
        matches!(self.lifecycle, IntentLifecycle::Active)
    }
}

/// An approved, source-linked revision of the native/projected goal.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct GoalRevision {
    pub revision: u64,
    pub text: String,
    pub source_refs: Vec<SourceRef>,
    pub supersedes_revision: Option<u64>,
    pub approval: Option<Approval>,
}

impl GoalRevision {
    #[must_use]
    pub fn new(revision: u64, text: impl Into<String>, source_refs: Vec<SourceRef>) -> Self {
        Self {
            revision,
            text: text.into(),
            source_refs,
            supersedes_revision: None,
            approval: None,
        }
    }
}

/// One alternative in a semantic conflict.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ConflictAlternative {
    pub id: String,
    pub text: String,
    pub source_refs: Vec<SourceRef>,
}

impl ConflictAlternative {
    #[must_use]
    pub fn new(
        id: impl Into<String>,
        text: impl Into<String>,
        source_refs: Vec<SourceRef>,
    ) -> Self {
        Self {
            id: id.into(),
            text: text.into(),
            source_refs,
        }
    }
}

/// A source-linked conflict affecting one or more active intent items.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Conflict {
    pub id: ConflictId,
    pub intent_ids: Vec<IntentId>,
    pub alternatives: Vec<ConflictAlternative>,
    pub source_refs: Vec<SourceRef>,
}

impl Conflict {
    #[must_use]
    pub fn new(
        id: impl Into<ConflictId>,
        intent_ids: Vec<IntentId>,
        alternatives: Vec<ConflictAlternative>,
        source_refs: Vec<SourceRef>,
    ) -> Self {
        Self {
            id: id.into(),
            intent_ids,
            alternatives,
            source_refs,
        }
    }
}

/// A source-linked choice that resolves one live conflict.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ConflictResolution {
    pub alternative_id: String,
    pub source_refs: Vec<SourceRef>,
    pub approval: Option<Approval>,
}

impl ConflictResolution {
    #[must_use]
    pub fn new(
        alternative_id: impl Into<String>,
        source_refs: Vec<SourceRef>,
        approval: Option<Approval>,
    ) -> Self {
        Self {
            alternative_id: alternative_id.into(),
            source_refs,
            approval,
        }
    }
}

fn is_false(value: &bool) -> bool {
    !value
}
