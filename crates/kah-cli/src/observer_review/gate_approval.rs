//! The user may close positive uncertainty, never a confirmed violation or
//! failed delivery. Proofs are produced by the host verification boundary.
use super::state::{Purpose, Request};
use crate::Result;
use kah_observer::{
    decision_units::{SemanticDecision, Thinking},
    review_evidence::Report,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use serde_json::json;
use std::path::Path;
mod question;
pub use question::{message, store};
mod resolution;
pub use resolution::resolve;
mod evidence;
pub use evidence::captured;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TestReceipt {
    pub command: String,
    pub commit: String,
    pub exit_code: Option<i32>,
    pub success: bool,
    #[serde(default)]
    pub output: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DeliveryProof {
    pub head: String,
    pub fresh_checkout: bool,
    pub tests: Vec<TestReceipt>,
    pub artifacts: Vec<kah_observer::decision_units::artifact::Receipt>,
}
impl DeliveryProof {
    pub fn passes(&self, head: &str) -> bool {
        oid(head)
            && self.head == head
            && self.fresh_checkout
            && !self.tests.is_empty()
            && !self.artifacts.is_empty()
            && self.tests.iter().all(|t| {
                t.commit == head
                    && t.exit_code == Some(0)
                    && t.success
                    && !t.command.trim().is_empty()
            })
            && self.artifacts.iter().all(|a| {
                oid(&a.baseline_blob)
                    && a.delivered_sha == head
                    && a.delivered_blob.as_deref() == Some(&a.baseline_blob)
            })
    }
}
fn oid(value: &str) -> bool {
    matches!(value.len(), 40 | 64) && value.bytes().all(|c| c.is_ascii_hexdigit())
}
pub fn eligible(
    request: &Request,
    report: &Report,
    assessment: &Value,
    proof: &DeliveryProof,
) -> bool {
    if request.purpose != Purpose::Checkpoint
        || !request.trigger["reconsideration"].is_object()
        || assessment.get("passed") != Some(&Value::Null)
        || !assessment["violations"]
            .as_array()
            .is_some_and(Vec::is_empty)
        || !assessment["needs_user"]
            .as_array()
            .is_some_and(Vec::is_empty)
        || !request.head.as_deref().is_some_and(|h| proof.passes(h))
    {
        return false;
    }
    let Some(units) = &report.decision_units else {
        return false;
    };
    assessment["needs_thinker"].as_array().is_some_and(|ids| {
        !ids.is_empty()
            && ids.iter().all(|id| {
                units
                    .requirements
                    .iter()
                    .find(|r| id == &r.id)
                    .is_some_and(|r| match &r.judgment {
                        Thinking::Semantic {
                            decision: SemanticDecision::Satisfied,
                            ..
                        } => true,
                        Thinking::AtomicFact { .. } => true,
                        Thinking::ArtifactPreservation { fact, .. } => fact.equal(),
                        _ => false,
                    })
            })
    })
}
