//! Byte preservation is an exact controller measurement, not a Jev inference.
use super::{Requirement, schema::object};
use crate::review_evidence::Document;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Receipt {
    pub path: String,
    pub baseline_blob: String,
    pub delivered_blob: Option<String>,
    pub delivered_sha: String,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ArtifactFact {
    pub receipt_citation: usize,
    pub receipt: Receipt,
}
impl ArtifactFact {
    pub fn equal(&self) -> bool {
        self.receipt.delivered_blob.as_deref() == Some(&self.receipt.baseline_blob)
    }
    pub(super) fn verify(&self, r: &Requirement, documents: &[Document]) -> Result<(), String> {
        let citation = r
            .citations
            .get(self.receipt_citation)
            .ok_or("REVIEW_ARTIFACT_CITATION stage=decision_units")?;
        let document=documents.iter().find(|d|d.id==citation.source && d.digest==citation.digest && d.kind=="controller_artifact_digests")
            .ok_or("REVIEW_ARTIFACT_PROVENANCE stage=decision_units: use a controller-produced digest receipt")?;
        let actual: Receipt = serde_json::from_str(&document.text)
            .map_err(|_| "REVIEW_ARTIFACT_RECEIPT stage=decision_units")?;
        if self.receipt != actual
            || !oid(&actual.baseline_blob)
            || !oid(&actual.delivered_sha)
            || actual.delivered_blob.as_ref().is_some_and(|s| !oid(s))
        {
            return Err("REVIEW_ARTIFACT_CHANGED stage=decision_units: digest/path/SHA must match the original controller receipt".into());
        }
        Ok(())
    }
}
fn oid(value: &str) -> bool {
    matches!(value.len(), 40 | 64) && value.bytes().all(|b| b.is_ascii_hexdigit())
}
pub(super) fn schema() -> Value {
    object(
        &["receipt_citation", "receipt"],
        json!({
            "receipt_citation":{"type":"integer","minimum":0},
            "receipt":object(&["path","baseline_blob","delivered_blob","delivered_sha"],json!({
                "path":{"type":"string"},"baseline_blob":{"type":"string"},
                "delivered_blob":{"type":["string","null"]},"delivered_sha":{"type":"string"}
            }))
        }),
    )
}
