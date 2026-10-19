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
mod stale;
pub use question::{message, store};
pub use stale::release as release_stale;
mod resolution;
pub use resolution::resolve;
mod evidence;
pub use evidence::captured;
mod receiver;
pub use receiver::{apply_answers, approved_authority, enqueue_answer, pending_answers};
mod checks;
mod contract_delivery;
mod contract_evidence;
mod contract_execution;
mod contract_facts;
mod contract_results;
pub mod interactive;
mod semantic_resolution;
pub(super) use contract_results::unchecked_final;
mod contracts;
pub(super) use contracts::{outcome as contract_outcome, prepare as prepare_contracts};
mod producer;
pub(super) use producer::{
    begin as begin_followup, finish as finish_followup, guard as guard_followup,
};
pub use producer::{
    pending as pending_followups, run as run_followups, run_with as run_followups_with,
};
pub mod followup;

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
    #[serde(default)]
    pub recorded_checks_absent: bool,
}
impl DeliveryProof {
    pub fn passes(&self, head: &str) -> bool {
        oid(head)
            && !self.recorded_checks_absent
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
    pub fn risk_receipts(&self, head: &str) -> bool {
        self.passes(head)
            || self.recorded_checks_absent
                && self.tests.is_empty()
                && self.head == head
                && oid(head)
                && self.fresh_checkout
                && self.artifacts.iter().all(|a| {
                    a.delivered_sha == head && a.delivered_blob.as_deref() == Some(&a.baseline_blob)
                })
    }
}
fn oid(value: &str) -> bool {
    matches!(value.len(), 40 | 64) && value.bytes().all(|c| c.is_ascii_hexdigit())
}
pub fn eligible(
    _request: &Request,
    _report: &Report,
    _assessment: &Value,
    _proof: &DeliveryProof,
) -> bool {
    // Native reports and trigger markers cannot admit a user risk decision.
    false
}

#[cfg(test)]
#[path = "../../tests/observer_unit/semantic_admission.rs"]
mod semantic_admission_tests;
#[cfg(test)]
#[path = "../../tests/observer_unit/semantic_obligation.rs"]
mod semantic_obligation_tests;
pub fn eligible_after_followup(
    request: &Request,
    report: &Report,
    assessment: &Value,
    proof: &DeliveryProof,
    followup: &followup::CompletedFollowup,
) -> bool {
    if !followup.matches(request)
        || report.request_id != request.id
        || !positive_uncertainty(request, report, assessment, proof)
    {
        return false;
    }
    let ids = assessment["needs_thinker"].as_array().unwrap();
    if ids.iter().any(|id| {
        id.as_str().is_some_and(|id| {
            semantic_resolution::semantic(report, id)
                && assessment["contract_coverage"][id]["semantic_risk_admissible"] != true
        })
    }) {
        return false;
    }
    if report
        .decision_units
        .as_ref()
        .is_some_and(|u| (10..12).contains(&u.version()))
        && ids.iter().any(|id| {
            let Some(id) = id.as_str() else {
                return true;
            };
            !report.verification_contracts.as_ref().is_some_and(|items| {
                items.iter().any(|i| {
                    i.requirement_id == id
                        && matches!(
                            i.method,
                            kah_observer::verification_contract::Method::SemanticRisk { .. }
                        )
                })
            }) || assessment["contract_coverage"][id]["semantic_risk_admissible"] != true
        })
    {
        return false;
    }
    let risks = followup.risks();
    risks.len() == ids.len()
        && ids
            .iter()
            .all(|id| risks.iter().filter(|r| id == &r.requirement_id).count() == 1)
        && risks
            .iter()
            .all(|r| ids.iter().filter(|id| *id == &r.requirement_id).count() == 1)
}
fn positive_uncertainty(
    request: &Request,
    report: &Report,
    assessment: &Value,
    proof: &DeliveryProof,
) -> bool {
    if request.purpose != Purpose::Checkpoint
        || assessment["state"] == "checkpoint_unconfirmed_observation"
        || assessment.get("passed") != Some(&Value::Null)
        || !assessment["violations"]
            .as_array()
            .is_some_and(Vec::is_empty)
        || !assessment["needs_user"]
            .as_array()
            .is_some_and(Vec::is_empty)
        || !request.head.as_deref().is_some_and(|h| {
            proof.passes(h) || interactive::usable_plan(report) && proof.risk_receipts(h)
        })
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
                        Thinking::AtomicFact { .. } => units.version() < 12,
                        Thinking::ArtifactPreservation { fact, .. } => {
                            units.version() < 12 && fact.equal()
                        }
                        _ => false,
                    })
            })
    })
}

pub(super) fn guard_report(
    root: &Path,
    request: &Request,
    report: &Report,
    assessment: &mut Value,
) -> Result<()> {
    if super::commit_packet::is_commit(request) {
        return Ok(());
    }
    interactive::guard(request, assessment)?;
    if captured(request)?.is_some_and(|p| p.recorded_checks_absent)
        && !request.trigger["interactive_gate_verification"].is_null()
    {
        return Ok(());
    }
    guard_followup(root, request, assessment)?;
    if semantic_resolution::guard(root, request, report, assessment)? {
        return Ok(());
    }
    unchecked_final(request, report, assessment)
}
