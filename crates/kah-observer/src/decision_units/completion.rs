//! Controller-owned receipts establish timing, never substantive success.
use super::{Requirement, schema::object};
use crate::{
    judge::{Answer, Judgment},
    review_evidence::Document,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Receipt {
    pub worker: String,
    pub request: String,
    pub head: Option<String>,
    pub event_id: String,
    pub event_kind: Kind,
    pub phase: String,
    pub requires_claim_verification: bool,
}
#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    UserGate,
    NativeGitTag,
    NativePhaseClaim,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct DueAtEvent {
    pub receipt_citation: usize,
    pub receipt: Receipt,
}
impl DueAtEvent {
    pub(super) fn verify(&self, r: &Requirement, documents: &[Document]) -> Result<(), String> {
        let citation = r
            .citations
            .get(self.receipt_citation)
            .ok_or("REVIEW_COMPLETION_RECEIPT_CITATION stage=decision_units")?;
        let doc = documents
            .iter()
            .find(|d| d.id == citation.source)
            .ok_or("REVIEW_COMPLETION_RECEIPT_SOURCE stage=decision_units")?;
        if doc.kind != "controller_completion_due" {
            return Err("REVIEW_COMPLETION_RECEIPT_SOURCE stage=decision_units: worker text is not a controller receipt".into());
        }
        let value: Value = serde_json::from_str(&doc.text)
            .map_err(|_| "REVIEW_COMPLETION_RECEIPT_JSON stage=decision_units")?;
        let receipt = serde_json::to_value(&self.receipt)
            .map_err(|_| "REVIEW_COMPLETION_RECEIPT_JSON stage=decision_units")?;
        if receipt["worker"] != value["worker"]
            || receipt["request"] != value["request"]
            || receipt["head"] != value["head"]
            || !value["receipts"]
                .as_array()
                .is_some_and(|items| items.contains(&receipt))
        {
            return Err("REVIEW_COMPLETION_RECEIPT_BINDING stage=decision_units: receipt must match this controller request, worker, SHA and event".into());
        }
        if self.receipt.requires_claim_verification
            && self.receipt.event_kind != Kind::NativePhaseClaim
        {
            return Err("REVIEW_COMPLETION_RECEIPT_TIMING stage=decision_units: explicit gates and tags are unconditional phase events".into());
        }
        Ok(())
    }
    pub(super) fn confirmed(&self, judgment: &Judgment) -> bool {
        !self.receipt.requires_claim_verification
            || judgment.backend == "jev"
                && matches!(judgment.answers.get("claim_made"),Some(Answer::Noul{noul}) if *noul>=crate::judge::questions::STEERING_PROBABILITY)
    }
}
pub(super) fn schema() -> Value {
    object(
        &["receipt_citation", "receipt"],
        json!({"receipt_citation":{"type":"integer","minimum":0},
        "receipt":object(&["worker","request","head","event_id","event_kind","phase","requires_claim_verification"],json!({
            "worker":{"type":"string"},"request":{"type":"string"},"head":{"type":["string","null"]},"event_id":{"type":"string"},
            "event_kind":{"enum":["user_gate","native_git_tag","native_phase_claim"]},"phase":{"type":"string"},"requires_claim_verification":{"type":"boolean"}
        }))}),
    )
}
