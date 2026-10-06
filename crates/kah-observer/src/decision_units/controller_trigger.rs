//! Host-recorded conditional triggers establish timing, never compliance.
use super::{Requirement, schema::object};
use crate::review_evidence::Document;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Trigger {
    pub receipt_citation: usize,
    pub receipt: Receipt,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Receipt {
    pub worker: String,
    pub request: String,
    pub head: Option<String>,
    pub event: Event,
    pub occurred: bool,
    pub receipts: Vec<Value>,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Event {
    ObserverEvidenceAsk,
}
impl Trigger {
    pub(super) fn verify(&self, r: &Requirement, documents: &[Document]) -> Result<(), String> {
        let a = r.applicability.as_ref().unwrap();
        if a.unconditional || a.controller_due.is_some() || a.due != self.receipt.occurred {
            return Err("REVIEW_TRIGGER_TIMING stage=decision_units: a conditional trigger cannot override an unconditional prohibition or phase receipt".into());
        }
        let citation = r
            .citations
            .get(self.receipt_citation)
            .ok_or("REVIEW_TRIGGER_CITATION stage=decision_units")?;
        let doc = documents
            .iter()
            .find(|d| d.id == citation.source)
            .ok_or("REVIEW_TRIGGER_SOURCE stage=decision_units")?;
        if doc.id != "controller-evidence" || doc.kind != "controller_event" {
            return Err("REVIEW_TRIGGER_SOURCE stage=decision_units: worker text cannot create a host receipt".into());
        }
        let value: Value = serde_json::from_str(&doc.text)
            .map_err(|_| "REVIEW_TRIGGER_JSON stage=decision_units")?;
        if serde_json::to_value(&self.receipt)
            .map_err(|_| "REVIEW_TRIGGER_JSON stage=decision_units")?
            != value
        {
            return Err("REVIEW_TRIGGER_BINDING stage=decision_units: match the host's worker, request, SHA and accepted delivery receipts".into());
        }
        Ok(())
    }
}
pub(super) fn schema() -> Value {
    object(
        &["receipt_citation", "receipt"],
        json!({
            "receipt_citation":{"type":"integer","minimum":0},
            "receipt":object(&["worker","request","head","event","occurred","receipts"], json!({
                "worker":{"type":"string"},"request":{"type":"string"},"head":{"type":["string","null"]},
                "event":{"const":"observer_evidence_ask"},"occurred":{"type":"boolean"},"receipts":{"type":"array","items":{"type":"object"}}
            }))
        }),
    )
}
