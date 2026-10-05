//! Host-recorded phase events supply literal timing receipts to the thinker.
use super::state::{Purpose, Request};
use crate::Result;
use kah_observer::{
    decision_units::completion::{Kind, Receipt},
    review_evidence::Document,
};
use serde_json::json;
use sha2::{Digest, Sha256};

pub fn add(request: &mut Request) -> Result<()> {
    if request.purpose != Purpose::Checkpoint {
        return Ok(());
    }
    let mut receipts = vec![];
    for event in request.trigger["events"].as_array().into_iter().flatten() {
        let kind = match event["kind"].as_str() {
            Some("user_gate") if event["source"] == "host_cli" => Kind::UserGate,
            Some("native_git_tag") if event["source"] == "controller Git metadata" => {
                Kind::NativeGitTag
            }
            Some("native_phase_claim") if confirmed_phase_claim(event) => Kind::NativePhaseClaim,
            _ => continue,
        };
        let id = format!("event-{:x}", Sha256::digest(serde_json::to_vec(event)?));
        receipts.push(Receipt {
            worker: request.worker.clone(),
            request: request.id.clone(),
            head: request.head.clone(),
            event_id: id,
            event_kind: kind,
            phase: "phase_at_event".into(),
            requires_claim_verification: kind == Kind::NativePhaseClaim
                && event["uncertain"] == true,
        });
    }
    if !receipts.is_empty() {
        request.documents.push(Document::new("completion-due","controller_completion_due","kah:controller-phase-events",&serde_json::to_string_pretty(&json!({"worker":request.worker,"request":request.id,"head":request.head,"receipts":receipts}))?));
    }
    Ok(())
}
fn confirmed_phase_claim(event: &serde_json::Value) -> bool {
    // The installed claim screen uses Noul, which has no separate confidence.
    // Its existing .90 probability gate must confirm the actual assertion;
    // backend identity alone cannot turn an interpretation into a known fact.
    event["judgment"]["backend"] == "jev"
        && event["uncertain"] != true
        && event["judgment"]["answers"]["phase_claim"]["noul"]
            .as_f64()
            .is_some_and(|p| p >= kah_observer::judge::questions::STEERING_PROBABILITY)
}
/// The full review can confirm a candidate that the initial screen could not.
pub fn reviewed_phase(request: &Request, judgment: &kah_observer::judge::Judgment) -> bool {
    request.purpose == Purpose::Checkpoint
        && super::checkpoint_claim::doubtful(request)
        && judgment.backend == "jev"
        && matches!(judgment.answers.get("claim_made"),Some(kah_observer::judge::Answer::Noul{noul}) if *noul>=kah_observer::judge::questions::STEERING_PROBABILITY)
}
/// A report cannot smuggle an older genuine receipt into a new decision.
pub fn bind(request: &Request, report: &kah_observer::review_evidence::Report) -> Result<()> {
    for r in report
        .decision_units
        .as_ref()
        .into_iter()
        .flat_map(|u| &u.requirements)
    {
        if let Some(fact) = r
            .applicability
            .as_ref()
            .and_then(|a| a.controller_due.as_ref())
            && (request.purpose != Purpose::Checkpoint
                || fact.receipt.worker != request.worker
                || fact.receipt.request != request.id
                || fact.receipt.head != request.head)
        {
            return Err("REVIEW_COMPLETION_RECEIPT_BINDING stage=report: receipt belongs to another worker, request or SHA".into());
        }
    }
    Ok(())
}
