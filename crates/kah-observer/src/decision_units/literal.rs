//! One receipt property per fact; semantic meaning belongs to the thinker.
use super::{CheckReceipt, schema::object};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

pub(super) const OPTIONS: [(&str, &str); 3] = [
    (
        "yes",
        "The verified native observation establishes this one property.",
    ),
    (
        "no",
        "The verified native observation contradicts this one property.",
    ),
    (
        "unclear",
        "The verified native observation cannot establish this property.",
    ),
];

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag = "property", rename_all = "snake_case", deny_unknown_fields)]
pub enum Literal {
    Sha { check: usize, expected: String },
    ExitStatus { check: usize, expected: i64 },
    OutputLine { check: usize, expected: String },
}
impl Literal {
    pub fn check(&self) -> usize {
        match self {
            Self::Sha { check, .. }
            | Self::ExitStatus { check, .. }
            | Self::OutputLine { check, .. } => *check,
        }
    }
    pub(super) fn validate(
        &self,
        checks: &[CheckReceipt],
        indices: &[usize],
    ) -> Result<(), String> {
        let receipt = checks
            .get(self.check())
            .ok_or("REVIEW_LITERAL_RECEIPT stage=decision_units: select one native check")?;
        let citation = match self {
            Self::Sha { expected, .. } => {
                if !matches!(expected.len(), 40 | 64)
                    || !expected.bytes().all(|b| b.is_ascii_hexdigit())
                {
                    return Err("REVIEW_LITERAL_SHA stage=decision_units".into());
                }
                receipt.sha_citation
            }
            Self::ExitStatus { .. } => receipt.result_citation,
            Self::OutputLine { expected, .. } => {
                if expected.trim().is_empty() || expected.contains(['\n', '\r']) {
                    return Err("REVIEW_LITERAL_LINE stage=decision_units: one complete output line per fact".into());
                }
                receipt.result_citation
            }
        };
        if !indices.contains(&citation) {
            return Err(
                "REVIEW_LITERAL_CITATION stage=decision_units: cite the selected receipt property"
                    .into(),
            );
        }
        Ok(())
    }
    pub(super) fn question(&self, reference: &str, checks: &[CheckReceipt]) -> String {
        let receipt = &checks[self.check()];
        match self {
            Self::Sha { expected, .. } => format!(
                "Does the verified native SHA observation at {reference}.citations[{}] report SHA {}?",
                receipt.sha_citation,
                json!(expected)
            ),
            Self::ExitStatus { expected, .. } => format!(
                "Does the verified native result at {reference}.citations[{}] report exit status {expected}?",
                receipt.result_citation
            ),
            Self::OutputLine { expected, .. } => format!(
                "Does the verified native result at {reference}.citations[{}] contain the complete output line {}?",
                receipt.result_citation,
                json!(expected)
            ),
        }
    }
}
pub(super) fn schema() -> Value {
    let fields = |property: &str, expected: Value| {
        object(
            &["property", "check", "expected"],
            json!({"property":{"const":property},"check":{"type":"integer","minimum":0},"expected":expected}),
        )
    };
    json!({"oneOf":[fields("sha",json!({"type":"string"})),fields("exit_status",json!({"type":"integer"})),fields("output_line",json!({"type":"string"}))]})
}
