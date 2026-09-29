use super::*;

#[derive(Debug)]
pub struct Ledger {
    pub(super) path: PathBuf,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Snapshot {
    pub(super) goal: String,
    pub(super) requirements: Vec<RequirementStatus>,
    pub(super) unresolved_requirement_ids: Vec<String>,
    pub(super) closed: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RequirementStatus {
    pub(super) id: String,
    pub(super) text: String,
    pub(super) evidence: Option<String>,
}

impl RequirementStatus {
    #[must_use]
    pub fn id(&self) -> &str {
        &self.id
    }

    #[must_use]
    pub fn text(&self) -> &str {
        &self.text
    }

    #[must_use]
    pub fn evidence(&self) -> Option<&str> {
        self.evidence.as_deref()
    }
}

impl Snapshot {
    #[must_use]
    pub fn goal(&self) -> &str {
        &self.goal
    }

    #[must_use]
    pub fn requirements(&self) -> &[RequirementStatus] {
        &self.requirements
    }

    #[must_use]
    pub fn unresolved_requirement_ids(&self) -> &[String] {
        &self.unresolved_requirement_ids
    }

    #[must_use]
    pub fn is_closed(&self) -> bool {
        self.closed
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LedgerError {
    pub code: &'static str,
    pub stage: &'static str,
    message: String,
}

impl LedgerError {
    pub(super) fn new(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            code,
            stage: "ledger",
            message: message.into(),
        }
    }
}

impl fmt::Display for LedgerError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "{} stage={}: {}",
            self.code, self.stage, self.message
        )
    }
}

impl std::error::Error for LedgerError {}

impl From<io::Error> for LedgerError {
    fn from(error: io::Error) -> Self {
        Self::new("LEDGER_IO", error.to_string())
    }
}

impl From<serde_json::Error> for LedgerError {
    fn from(error: serde_json::Error) -> Self {
        Self::new("LEDGER_JSON", error.to_string())
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ClosureError {
    UnresolvedRequirements(Vec<String>),
    Ledger(String),
}

impl fmt::Display for ClosureError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnresolvedRequirements(ids) => {
                write!(
                    formatter,
                    "UNRESOLVED_REQUIREMENTS stage=closure: {}",
                    ids.join(", ")
                )
            }
            Self::Ledger(message) => formatter.write_str(message),
        }
    }
}

impl std::error::Error for ClosureError {}

impl From<LedgerError> for ClosureError {
    fn from(error: LedgerError) -> Self {
        Self::Ledger(error.to_string())
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub(super) struct Requirement {
    pub(super) id: String,
    pub(super) text: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub(super) struct Record {
    pub(super) schema_version: u32,
    pub(super) sequence: u64,
    #[serde(flatten)]
    pub(super) event: Event,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub(super) enum Event {
    RunStarted {
        goal: String,
        requirements: Vec<Requirement>,
    },
    SteeringAdded {
        requirement: Requirement,
    },
    RequirementSatisfied {
        requirement_id: String,
        evidence: String,
    },
    RunClosed,
}

#[derive(Debug)]
pub(super) struct FoldedState {
    pub(super) goal: String,
    pub(super) requirements: BTreeMap<String, RequirementState>,
    pub(super) closed: bool,
}

#[derive(Debug)]
pub(super) struct RequirementState {
    pub(super) text: String,
    pub(super) evidence: Option<String>,
}
