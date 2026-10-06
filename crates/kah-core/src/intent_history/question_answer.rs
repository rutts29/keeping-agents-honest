use super::*;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum GateDecision {
    Approve,
    Reject,
}
impl GateDecision {
    pub fn parse(value: &str) -> Result<Self, HistoryError> {
        match value {
            "approve" => Ok(Self::Approve),
            "reject" => Ok(Self::Reject),
            _ => Err(HistoryError::InvalidSourceRef {
                field: "question answer decision",
            }),
        }
    }
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Approve => "approve",
            Self::Reject => "reject",
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct QuestionAnswer {
    pub question: String,
    pub decision: GateDecision,
}
impl QuestionAnswer {
    pub fn validate(&self) -> Result<(), HistoryError> {
        if self
            .question
            .strip_prefix("question-")
            .is_none_or(|id| id.len() != 64 || !id.bytes().all(|c| c.is_ascii_hexdigit()))
        {
            return Err(HistoryError::InvalidSourceRef {
                field: "question answer id",
            });
        }
        Ok(())
    }
}
