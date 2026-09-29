use super::*;

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize)]
pub struct ResolverUsage {
    pub input_tokens: u64,
    pub cached_input_tokens: u64,
    pub output_tokens: u64,
    pub reasoning_output_tokens: u64,
}

impl ResolverUsage {
    pub(super) fn add(&mut self, other: &Self) {
        self.input_tokens = self.input_tokens.saturating_add(other.input_tokens);
        self.cached_input_tokens = self
            .cached_input_tokens
            .saturating_add(other.cached_input_tokens);
        self.output_tokens = self.output_tokens.saturating_add(other.output_tokens);
        self.reasoning_output_tokens = self
            .reasoning_output_tokens
            .saturating_add(other.reasoning_output_tokens);
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ResolverMetadata {
    pub model: String,
    pub reasoning: String,
    pub calls: u8,
    pub elapsed_ms: u128,
    pub usage: ResolverUsage,
    pub usage_complete: bool,
    pub prompt_schema_version: u32,
    pub proposal_schema_version: u32,
    pub last_validation_failure: Option<String>,
    pub artifact_ids: Vec<String>,
}

#[derive(Debug)]
pub struct InspectResolution {
    pub history: History,
    pub projection: ActiveProjection,
    pub metadata: ResolverMetadata,
    pub native_goal: NativeGoalObservation,
    pub goal_change: Option<GoalChangeObservation>,
}

impl InspectResolution {
    pub fn continuation_blocked(&self) -> bool {
        self.projection.continuation_blocked()
            || self.native_goal.conflicts_with_projection
            || self.goal_change.is_some()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NativeGoalObservation {
    pub state: String,
    /// Local-only observed text. Sanitized renderers expose only state and
    /// whether it differs from accepted projected intent.
    pub text_private: Option<String>,
    pub conflicts_with_projection: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GoalChangeObservation {
    pub proposed_goal: String,
    pub source_refs: Vec<SourceRef>,
    pub base_projection_revision: u64,
    pub base_event_sequence: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ResolverFailureKind {
    Execution,
    InvalidProposal,
}

#[derive(Debug)]
pub struct ResolverFailure {
    pub code: String,
    pub stage: &'static str,
    pub kind: ResolverFailureKind,
    pub metadata: Box<ResolverMetadata>,
}

impl fmt::Display for ResolverFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{} stage={}: ", self.code, self.stage)?;
        match self.kind {
            ResolverFailureKind::Execution => {
                formatter.write_str("Codex compactor execution failed")
            }
            ResolverFailureKind::InvalidProposal => {
                formatter.write_str("Codex compactor returned an invalid proposal after repair")
            }
        }
    }
}

impl std::error::Error for ResolverFailure {}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ProjectionProposal {
    pub(super) schema_version: u32,
    pub(super) goal: GoalProposal,
    pub(super) accounted_source_record_ids: Vec<String>,
    pub(super) operations: Vec<OperationProposal>,
}

#[derive(Clone, Debug)]
pub(super) struct RepairContext {
    pub(super) failure: String,
    pub(super) previous_proposal: Option<Value>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct IncrementalProjectionProposal {
    pub(super) schema_version: u32,
    pub(super) base_projection_revision: u64,
    pub(super) base_event_sequence: u64,
    pub(super) classification: IncrementalClassification,
    pub(super) accounted_active_intent_ids: Vec<String>,
    pub(super) accounted_source_record_ids: Vec<String>,
    pub(super) operations: Vec<IncrementalOperationProposal>,
    #[serde(default)]
    pub(super) proposed_goal: Option<IncrementalGoalProposal>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct IncrementalGoalProposal {
    pub(super) text: String,
    pub(super) source_record_ids: Vec<String>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum IncrementalClassification {
    NoChange,
    Additive,
    Supersession,
    Withdrawal,
    Conflict,
    Reopen,
    GoalChange,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum IncrementalOperationName {
    Add,
    Supersede,
    Withdraw,
    Conflict,
    Reopen,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct IncrementalOperationProposal {
    pub(super) operation: IncrementalOperationName,
    pub(super) key: String,
    pub(super) kind: IntentKind,
    pub(super) text: String,
    pub(super) target_intent_id: String,
    pub(super) intent_ids: Vec<String>,
    pub(super) evidence_id: String,
    pub(super) reason: String,
    pub(super) source_record_ids: Vec<String>,
    pub(super) alternatives: Vec<AlternativeProposal>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct GoalProposal {
    pub(super) text: String,
    pub(super) source_record_ids: Vec<String>,
}

#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum OperationName {
    Add,
    Supersede,
    Withdraw,
    Conflict,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct OperationProposal {
    pub(super) operation: OperationName,
    pub(super) key: String,
    pub(super) kind: IntentKind,
    pub(super) text: String,
    pub(super) target_key: String,
    pub(super) intent_keys: Vec<String>,
    pub(super) source_record_ids: Vec<String>,
    pub(super) alternatives: Vec<AlternativeProposal>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct AlternativeProposal {
    pub(super) key: String,
    pub(super) text: String,
    pub(super) source_record_ids: Vec<String>,
}

pub(super) struct CodexCall {
    pub(super) final_message: String,
    pub(super) usage: ResolverUsage,
    pub(super) artifact_id: String,
}

#[derive(Clone, Copy, Debug)]
pub(super) enum ValidationFailure {
    SchemaVersion,
    EmptyGoal,
    EmptyOperations,
    InvalidKey,
    EmptyClause,
    AddIrrelevantFields,
    SupersedeShape,
    WithdrawShape,
    ConflictShape,
    DuplicateOperationKey,
    AlternativeShape,
    Source,
    Transition,
    Projection,
    StaleBase,
    ActiveAccounting,
    DeltaBound,
    GoalChange,
}

impl ValidationFailure {
    pub(super) fn code(self) -> &'static str {
        match self {
            Self::SchemaVersion => "unsupported_proposal_schema_version",
            Self::EmptyGoal => "empty_goal",
            Self::EmptyOperations => "empty_operations",
            Self::InvalidKey => "invalid_semantic_key",
            Self::EmptyClause => "empty_clause",
            Self::AddIrrelevantFields => "add_irrelevant_fields",
            Self::SupersedeShape => "supersede_shape",
            Self::WithdrawShape => "withdraw_shape",
            Self::ConflictShape => "conflict_shape",
            Self::DuplicateOperationKey => "duplicate_operation_key",
            Self::AlternativeShape => "alternative_shape",
            Self::Source => "source_accounting_or_authority",
            Self::Transition => "invalid_history_transition",
            Self::Projection => "projection_build",
            Self::StaleBase => "stale_incremental_base",
            Self::ActiveAccounting => "active_intent_accounting",
            Self::DeltaBound => "incremental_delta_bound",
            Self::GoalChange => "invalid_goal_change_proposal",
        }
    }
}
