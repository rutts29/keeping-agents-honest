use super::*;

/// A replay failure.  Errors distinguish malformed envelopes from invalid
/// domain transitions so callers can report a useful source-boundary issue.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum HistoryError {
    EmptyHistory,
    UnsupportedSchemaVersion { expected: u32, found: u32 },
    SequenceGap { expected: u64, found: u64 },
    RunStartedRequired,
    RunStartedNotFirst,
    EventAfterClosure,
    DuplicateId { kind: &'static str, id: String },
    UnknownIntent { id: IntentId },
    UnknownConflict { id: ConflictId },
    UnknownEvidence { id: EvidenceId },
    MissingSourceRefs { event: &'static str },
    InvalidSourceRef { field: &'static str },
    InvalidQuestionAnswer { reason: &'static str },
    UnauthorizedSource { event: &'static str },
    InvalidApproval { event: &'static str },
    InvalidTransition { event: &'static str, reason: String },
    Serialization(String),
}

impl fmt::Display for HistoryError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{} stage=history: ", self.code())?;
        match self {
            Self::EmptyHistory => formatter.write_str("intent history is empty"),
            Self::UnsupportedSchemaVersion { expected, found } => write!(
                formatter,
                "unsupported intent history schema version: expected {expected}, found {found}"
            ),
            Self::SequenceGap { expected, found } => {
                write!(
                    formatter,
                    "invalid intent history sequence: expected {expected}, found {found}"
                )
            }
            Self::RunStartedRequired => {
                formatter.write_str("intent history must begin with run_started")
            }
            Self::RunStartedNotFirst => formatter.write_str("run_started may only appear first"),
            Self::EventAfterClosure => {
                formatter.write_str("intent history contains an event after closure")
            }
            Self::DuplicateId { kind, id } => write!(formatter, "duplicate {kind} id: {id}"),
            Self::UnknownIntent { id } => write!(formatter, "unknown intent id: {id}"),
            Self::UnknownConflict { id } => write!(formatter, "unknown conflict id: {id}"),
            Self::UnknownEvidence { id } => write!(formatter, "unknown evidence id: {id}"),
            Self::MissingSourceRefs { event } => {
                write!(formatter, "{event} requires nonempty source refs")
            }
            Self::InvalidSourceRef { field } => write!(formatter, "{field} must not be empty"),
            Self::InvalidQuestionAnswer { reason } => formatter.write_str(reason),
            Self::UnauthorizedSource { event } => write!(
                formatter,
                "{event} requires a user source or explicit operator approval"
            ),
            Self::InvalidApproval { event } => {
                write!(formatter, "{event} has no valid explicit approval source")
            }
            Self::InvalidTransition { event, reason } => write!(formatter, "{event}: {reason}"),
            Self::Serialization(message) => formatter.write_str(message),
        }
    }
}

impl std::error::Error for HistoryError {}

impl From<serde_json::Error> for HistoryError {
    fn from(error: serde_json::Error) -> Self {
        Self::Serialization(error.to_string())
    }
}

impl HistoryError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::EmptyHistory => "HISTORY_EMPTY",
            Self::UnsupportedSchemaVersion { .. } => "HISTORY_SCHEMA",
            Self::SequenceGap { .. } => "HISTORY_SEQUENCE",
            Self::RunStartedRequired => "HISTORY_START_MISSING",
            Self::RunStartedNotFirst => "HISTORY_START_REPEATED",
            Self::EventAfterClosure => "HISTORY_AFTER_CLOSURE",
            Self::DuplicateId { .. } => "HISTORY_DUPLICATE",
            Self::UnknownIntent { .. } => "HISTORY_UNKNOWN_INTENT",
            Self::UnknownConflict { .. } => "HISTORY_UNKNOWN_CONFLICT",
            Self::UnknownEvidence { .. } => "HISTORY_UNKNOWN_EVIDENCE",
            Self::MissingSourceRefs { .. } => "HISTORY_SOURCES_MISSING",
            Self::InvalidSourceRef { .. } | Self::InvalidQuestionAnswer { .. } => {
                "HISTORY_SOURCE_INVALID"
            }
            Self::UnauthorizedSource { .. } => "HISTORY_AUTHORITY",
            Self::InvalidApproval { .. } => "HISTORY_APPROVAL",
            Self::InvalidTransition { .. } => "HISTORY_TRANSITION",
            Self::Serialization(..) => "HISTORY_JSON",
        }
    }
}
