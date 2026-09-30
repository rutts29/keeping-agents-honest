use super::*;

/// The schema version of an active projection payload.
pub const PROJECTION_SCHEMA_VERSION: u32 = 1;

/// The conservative default used by callers that do not choose a budget.
pub const DEFAULT_PROJECTION_BYTE_BUDGET: usize = 32 * 1024;

/// Configuration for deterministic projection rendering.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ProjectionConfig {
    /// Maximum number of bytes in the serialized prompt payload.
    pub byte_budget: usize,
}

impl ProjectionConfig {
    #[must_use]
    pub const fn new(byte_budget: usize) -> Self {
        Self { byte_budget }
    }
}

impl Default for ProjectionConfig {
    fn default() -> Self {
        Self::new(DEFAULT_PROJECTION_BYTE_BUDGET)
    }
}

impl From<usize> for ProjectionConfig {
    fn from(byte_budget: usize) -> Self {
        Self::new(byte_budget)
    }
}

/// The source position represented by a projection.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ProjectionSourceHead {
    /// The latest immutable history event sequence, represented as a stable
    /// local source-record label.
    pub record: String,
    /// The latest event sequence included in the projection.
    pub pending_sequence: u64,
}

/// Static provenance for the deterministic projection builder.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ProjectionGeneratedBy {
    pub component: String,
    pub model: Option<String>,
    pub reasoning: Option<String>,
    pub prompt_schema_version: u32,
}

/// The reason a closure attempt remains blocked.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ClosureBlockerKind {
    UnresolvedRequirement,
    Conflict,
    ProjectionOverflow,
}

/// A deterministic, source-linked closure blocker.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ClosureBlocker {
    pub kind: ClosureBlockerKind,
    /// Intent or conflict identifier, when the blocker has one.
    pub id: Option<String>,
    pub reason: String,
    pub source_refs: Vec<SourceRef>,
}

/// Explicit closure state derived from the active projection.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ClosureState {
    pub blockers: Vec<ClosureBlocker>,
}

impl ClosureState {
    #[must_use]
    pub fn is_blocked(&self) -> bool {
        !self.blockers.is_empty()
    }

    #[must_use]
    pub fn can_close(&self) -> bool {
        !self.is_blocked()
    }
}

/// Byte-accounting for the rendered prompt payload.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ProjectionOverflow {
    /// Exact UTF-8 byte length of the serialized prompt payload.
    pub bytes: usize,
    pub budget: usize,
    /// Number of intent/conflict items represented by that payload.
    pub item_count: usize,
    /// `None` means the payload is within budget; `Some` is a blocking reason.
    pub reason: Option<String>,
}

impl ProjectionOverflow {
    #[must_use]
    pub fn is_overflow(&self) -> bool {
        self.reason.is_some()
    }
}

/// The bounded active intent projection consumed by a continuation prompt.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ActiveProjection {
    /// Optional prompt-only synthesis; canonical items and goal remain verbatim.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub compaction: Option<CompactedIntent>,
    pub schema_version: u32,
    pub revision: u64,
    pub source_head: ProjectionSourceHead,
    pub goal: GoalRevision,
    pub preserve: Vec<IntentItem>,
    pub frontier: Vec<IntentItem>,
    pub validation: Vec<IntentItem>,
    pub conflicts: Vec<Conflict>,
    pub closure: ClosureState,
    pub overflow: ProjectionOverflow,
    pub generated_by: ProjectionGeneratedBy,
}

impl ActiveProjection {
    /// Build a projection with either a [`ProjectionConfig`] or a byte budget.
    pub fn from_history<C>(history: &History, config: C) -> Result<Self, ProjectionError>
    where
        C: Into<ProjectionConfig>,
    {
        project(history, config)
    }

    /// Serialize exactly the prompt payload whose bytes are budgeted.
    pub fn try_rendered_prompt(&self) -> Result<String, ProjectionError> {
        if let Some(compaction) = &self.compaction {
            return serde_json::to_string(&serde_json::json!({
                "goal": self.goal, "compacted_intent": compaction,
                "review_required": self.needs_review(),
                "authority": "Goal is original user text. Active steering supersedes historical details. Summary is an interpretation; retrieve originals from kah ledger before resolving ambiguity."
            })).map_err(ProjectionError::from);
        }
        serde_json::to_string(&PromptPayload {
            schema_version: self.schema_version,
            goal: &self.goal,
            preserve: &self.preserve,
            frontier: &self.frontier,
            validation: &self.validation,
            conflicts: &self.conflicts,
        })
        .map_err(ProjectionError::from)
    }

    /// Serialize exactly the prompt payload whose bytes are budgeted.
    #[must_use]
    pub fn rendered_prompt(&self) -> String {
        self.try_rendered_prompt()
            .expect("active projection prompt serialization must be infallible")
    }

    /// Return the exact UTF-8 byte length of [`Self::rendered_prompt`].
    #[must_use]
    pub fn rendered_bytes(&self) -> usize {
        self.rendered_prompt().len()
    }

    #[must_use]
    pub fn is_overflowed(&self) -> bool {
        self.overflow.is_overflow()
    }

    #[must_use]
    pub fn needs_review(&self) -> bool {
        self.preserve
            .iter()
            .chain(&self.frontier)
            .chain(&self.validation)
            .any(|i| i.review_required || !i.uncertain_supersedes.is_empty())
    }

    #[must_use]
    pub fn continuation_blocked(&self) -> bool {
        !self.conflicts.is_empty() || self.overflow.is_overflow()
    }
}

/// A failure to account for history state in a projection.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ProjectionError {
    MissingAccountability {
        context: String,
    },
    InvalidState {
        reason: String,
    },
    InvalidSource {
        context: String,
        source: HistoryError,
    },
    Serialization(String),
}

impl fmt::Display for ProjectionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{} stage=projection: ", self.code())?;
        match self {
            Self::MissingAccountability { context } => {
                write!(formatter, "projection accountability is missing: {context}")
            }
            Self::InvalidState { reason } => {
                write!(formatter, "invalid projection state: {reason}")
            }
            Self::InvalidSource { context, source } => {
                write!(
                    formatter,
                    "invalid projection source for {context}: {source}"
                )
            }
            Self::Serialization(message) => {
                write!(formatter, "projection serialization failed: {message}")
            }
        }
    }
}

impl std::error::Error for ProjectionError {}

impl From<serde_json::Error> for ProjectionError {
    fn from(error: serde_json::Error) -> Self {
        Self::Serialization(error.to_string())
    }
}

impl From<HistoryError> for ProjectionError {
    fn from(error: HistoryError) -> Self {
        Self::InvalidSource {
            context: "history".to_owned(),
            source: error,
        }
    }
}

#[derive(Serialize)]
pub(super) struct PromptPayload<'a> {
    pub(super) schema_version: u32,
    pub(super) goal: &'a GoalRevision,
    pub(super) preserve: &'a [IntentItem],
    pub(super) frontier: &'a [IntentItem],
    pub(super) validation: &'a [IntentItem],
    pub(super) conflicts: &'a [Conflict],
}

impl ProjectionError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::MissingAccountability { .. } => "PROJECTION_ACCOUNTABILITY",
            Self::InvalidState { .. } => "PROJECTION_STATE",
            Self::InvalidSource { .. } => "PROJECTION_SOURCE",
            Self::Serialization(_) => "PROJECTION_JSON",
        }
    }
}

/// A bounded interpretation, linked to every verbatim input by item ID and digest.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct CompactedIntent {
    pub source_digest: String,
    pub summary: String,
    pub originals: Vec<String>,
    pub model: String,
}
