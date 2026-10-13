use super::*;

/// The one canonical question id: `question-` and the lowercase hex digest.
/// Every non-gate creator uses this; the frozen gate creator uses the same form.
#[must_use]
pub fn question_id(digest: &[u8]) -> String {
    let hex = digest
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect::<String>();
    format!("question-{hex}")
}
/// Accepts the canonical id and the legacy `question-sha256:<hex>` id that
/// rule-conflict questions stored and printed before the canonical form.
#[must_use]
pub fn is_question_id(id: &str) -> bool {
    id.strip_prefix("question-")
        .map(|rest| rest.strip_prefix("sha256:").unwrap_or(rest))
        .is_some_and(|hex| hex.len() == 64 && hex.bytes().all(|c| c.is_ascii_hexdigit()))
}
/// Rejects any id that is not a printed question id, with the accepted forms.
pub fn validate_question_id(id: &str) -> Result<(), HistoryError> {
    if is_question_id(id) {
        Ok(())
    } else {
        Err(invalid(ID_FORMAT))
    }
}
fn invalid(reason: &'static str) -> HistoryError {
    HistoryError::InvalidQuestionAnswer { reason }
}
const ID_FORMAT: &str = "question answer id must be question-<64 hex> or question-sha256:<64 hex> as the alert prints it";

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
            _ => Err(invalid("question answer must be approve or reject")),
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
        validate_question_id(&self.question)
    }
}
/// A relayed user choice of one stored option of an option question.
/// `option` is the 1-based position; `text` is that option's exact text.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct QuestionChoice {
    pub question: String,
    pub option: usize,
    pub text: String,
}
impl QuestionChoice {
    pub fn validate(&self) -> Result<(), HistoryError> {
        validate_question_id(&self.question)?;
        if self.option == 0 || self.text.trim().is_empty() {
            return Err(invalid(
                "question choice needs a 1-based option number and its exact option text",
            ));
        }
        Ok(())
    }
}
