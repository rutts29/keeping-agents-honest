use super::*;

/// Recoverable failures at the local filesystem boundary.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RunStoreError {
    InvalidRunId,
    Busy,
    RunAlreadyExists,
    RunNotFound,
    SymlinkRefused {
        path: PathBuf,
    },
    InvalidStateComponent {
        path: PathBuf,
    },
    ImmutableHistorySegment {
        path: PathBuf,
    },
    PrivateStatePermissions {
        path: PathBuf,
    },
    ReviewAlreadyRecorded,
    ProjectionMismatch,
    Io {
        action: &'static str,
        path: PathBuf,
        message: String,
    },
    History(HistoryError),
    Projection(ProjectionError),
    SourceCursor(SourceCursorError),
    Serialization(String),
}

impl fmt::Display for RunStoreError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{} stage=run_store: ", self.code())?;
        match self {
            Self::InvalidRunId => formatter.write_str("invalid opaque run id"),
            Self::Busy => formatter.write_str("run store is busy"),
            Self::RunAlreadyExists => formatter.write_str("run store already exists"),
            Self::RunNotFound => formatter.write_str("run store does not exist"),
            Self::SymlinkRefused { path } => {
                write!(
                    formatter,
                    "refusing symlinked state component: {}",
                    path.display()
                )
            }
            Self::InvalidStateComponent { path } => {
                write!(formatter, "invalid state component: {}", path.display())
            }
            Self::ImmutableHistorySegment { path } => {
                write!(
                    formatter,
                    "immutable history segment already exists: {}",
                    path.display()
                )
            }
            Self::PrivateStatePermissions { path } => {
                write!(
                    formatter,
                    "private state file has unsafe permissions: {}",
                    path.display()
                )
            }
            Self::ReviewAlreadyRecorded => formatter.write_str(
                "review is already recorded for this candidate checkpoint; change the candidate before reviewing again",
            ),
            Self::ProjectionMismatch => {
                formatter.write_str("stored projection does not match validated history")
            }
            Self::Io {
                action,
                path,
                message,
            } => write!(formatter, "{action} {}: {message}", path.display()),
            Self::History(error) => error.fmt(formatter),
            Self::Projection(error) => error.fmt(formatter),
            Self::SourceCursor(error) => error.fmt(formatter),
            Self::Serialization(message) => formatter.write_str(message),
        }
    }
}

impl std::error::Error for RunStoreError {}

impl From<HistoryError> for RunStoreError {
    fn from(error: HistoryError) -> Self {
        Self::History(error)
    }
}

impl From<ProjectionError> for RunStoreError {
    fn from(error: ProjectionError) -> Self {
        Self::Projection(error)
    }
}

impl From<SourceCursorError> for RunStoreError {
    fn from(error: SourceCursorError) -> Self {
        Self::SourceCursor(error)
    }
}

impl RunStoreError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::InvalidRunId => "STORE_INVALID_RUN_ID",
            Self::Busy => "STORE_BUSY",
            Self::RunAlreadyExists => "STORE_RUN_ALREADY_EXISTS",
            Self::RunNotFound => "STORE_RUN_NOT_FOUND",
            Self::SymlinkRefused { .. } => "STORE_SYMLINK_REFUSED",
            Self::InvalidStateComponent { .. } => "STORE_INVALID_STATE_COMPONENT",
            Self::ImmutableHistorySegment { .. } => "STORE_IMMUTABLE_HISTORY_SEGMENT",
            Self::PrivateStatePermissions { .. } => "STORE_PRIVATE_STATE_PERMISSIONS",
            Self::ReviewAlreadyRecorded => "STORE_REVIEW_ALREADY_RECORDED",
            Self::ProjectionMismatch => "STORE_PROJECTION_MISMATCH",
            Self::Io { .. } => "STORE_IO",
            Self::History(..) => "STORE_HISTORY",
            Self::Projection(..) => "STORE_PROJECTION",
            Self::SourceCursor(..) => "STORE_SOURCE_CURSOR",
            Self::Serialization(..) => "STORE_SERIALIZATION",
        }
    }
}
