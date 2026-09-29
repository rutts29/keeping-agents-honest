use super::*;

/// State reconstructed after reopening a run.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RecoveredRun {
    /// The last atomically committed active projection.
    pub projection: ActiveProjection,
    /// All events represented by immutable history plus the unrotated delta.
    pub history: History,
    /// Events still held in `pending.jsonl`.
    pub pending_delta: Vec<EventRecord>,
    /// The optional private source cursor.  Runs created before this state
    /// record existed remain readable without an implicit migration.
    pub source_cursor: Option<SourceCursor>,
}

/// Private identity of the one continued child accepted for run-bound checks.
#[derive(Clone, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CandidateBinding {
    pub(super) schema_version: u32,
    pub(super) child_thread_id: String,
    pub(super) candidate_path: PathBuf,
    pub(super) approved_goal_digest: String,
}

impl fmt::Debug for CandidateBinding {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("CandidateBinding")
            .field("schema_version", &self.schema_version)
            .field("child_thread_id", &self.child_thread_id)
            .finish_non_exhaustive()
    }
}

impl CandidateBinding {
    #[must_use]
    pub fn child_thread_id(&self) -> &str {
        &self.child_thread_id
    }

    /// The candidate path is private local state and must not enter a public bundle.
    #[must_use]
    pub fn candidate_path_private(&self) -> &Path {
        &self.candidate_path
    }

    #[must_use]
    pub fn approved_goal_digest(&self) -> &str {
        &self.approved_goal_digest
    }

    #[must_use]
    pub fn matches_approved_goal(&self, objective: &str) -> bool {
        let mut hasher = Sha256::new();
        hasher.update(b"driftctl.approved-child-goal.v1\0");
        hasher.update(objective.as_bytes());
        self.approved_goal_digest == format!("sha256:{:x}", hasher.finalize())
    }
}

/// Required run-level evidence gates beyond per-requirement checks.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CompletionGate {
    Regression,
    Integration,
    ProtectedScope,
    Review,
}

impl CompletionGate {
    pub const ALL: [Self; 4] = [
        Self::Regression,
        Self::Integration,
        Self::ProtectedScope,
        Self::Review,
    ];

    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Regression => "regression",
            Self::Integration => "integration",
            Self::ProtectedScope => "protected_scope",
            Self::Review => "review",
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CompletionGateRecord {
    pub gate: CompletionGate,
    pub passed: bool,
    pub status: String,
    pub candidate_digest: String,
    pub artifact_id: String,
    pub command_digest: String,
    pub verifier_digest: String,
    pub stdout_digest: String,
    pub stderr_digest: String,
    pub started_at_unix_ms: u128,
    pub elapsed_ms: u128,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CompletionGateState {
    pub(super) schema_version: u32,
    pub(super) records: BTreeMap<CompletionGate, CompletionGateRecord>,
}

impl CompletionGateState {
    pub(super) fn empty() -> Self {
        Self {
            schema_version: COMPLETION_GATE_STATE_SCHEMA_VERSION,
            records: BTreeMap::new(),
        }
    }

    #[must_use]
    pub fn record(&self, gate: CompletionGate) -> Option<&CompletionGateRecord> {
        self.records.get(&gate)
    }
}
