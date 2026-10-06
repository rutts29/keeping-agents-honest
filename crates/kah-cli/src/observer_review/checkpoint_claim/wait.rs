//! Status uncertainty is observer work, never a user decision.
use super::*;
use crate::{
    Result,
    observer_review::{source_store, state::Store},
};
use kah_observer::review_evidence::Report;
use sha2::{Digest, Sha256};

pub fn release_waiting(
    store: &Store,
    state: &mut crate::observer_review::state::State,
) -> Result<bool> {
    let Some(pending) = &state.pending else {
        return Ok(false);
    };
    if !waiting(&state.worker.session, &pending.id)? {
        return Ok(false);
    }
    state.pending = None;
    store.save(state)?;
    Ok(true)
}

pub fn waiting(worker: &str, request: &str) -> Result<bool> {
    let root = Store::directory(worker)?;
    let parked: Option<Value> =
        crate::observer_review::state::read(&root.join("parked").join(format!("{request}.json")))?;
    if parked
        .as_ref()
        .is_some_and(|p| p["unconfirmed_observation"] == true)
    {
        return Ok(true);
    }
    let Some(id) = parked.as_ref().and_then(|p| p["question"].as_str()) else {
        return Ok(false);
    };
    if !id.starts_with("question-") || !id.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'-') {
        return Err("REVIEW_QUESTION_ID stage=claim_wait".into());
    }
    let question: Option<Value> =
        crate::observer_review::state::read(&root.join("questions").join(format!("{id}.json")))?;
    Ok(question.is_some_and(|q| q["status"] == "pending"))
}

pub fn record_uncertainty(
    request: &Request,
    report: &Report,
    judgment: &Judgment,
) -> Result<Option<Value>> {
    let root = Store::directory(&request.worker)?;
    let history = root.join("claim-uncertainty");
    kah_core::run_store::ensure_private_directory(&history)?;
    let name = format!("{}.json", request.id);
    let previous: Option<Value> = crate::observer_review::state::read(&history.join(&name))?;
    let digest = kah_observer::context_selection::digest(&serde_json::to_value(report)?);
    let redelivery: Value = crate::observer_review::state::read(&root.join("redelivery.json"))?
        .unwrap_or_else(|| json!({}));
    let submitted = redelivery["requests"][&request.id]["attempts"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|a| a["status"] == "submitted_waiting_for_report")
        .count();
    let rethink = previous.as_ref().is_some_and(|p| {
        p["report_digest"] != digest
            || p["submitted_reasks"]
                .as_u64()
                .is_some_and(|n| submitted as u64 > n)
    });
    let record = json!({"request":request.id,"report_digest":digest,"submitted_reasks":submitted,
        "judgment":judgment,"at_ms":kah_observer::session::now_ms(),"thinker_reconsidered":rethink});
    if previous.is_none() {
        source_store::save(&history, &name, &record)?;
        return Ok(None);
    }
    if !rethink && previous.as_ref().is_none_or(|p| p["observation"].is_null()) {
        return Ok(None);
    }
    let observation = observation(request, report, judgment);
    let directory = root.join("claim-observations");
    kah_core::run_store::ensure_private_directory(&directory)?;
    let _lock = Store::delivery_at(&directory, true)?;
    let qname = format!("{}.json", observation["id"].as_str().unwrap());
    if !directory.join(&qname).exists() {
        source_store::save(&directory, &qname, &observation)?;
    }
    let mut record = record;
    record["observation"] = observation["id"].clone();
    source_store::save(&history, &name, &record)?;
    kah_core::run_store::ensure_private_directory(&root.join("parked"))?;
    source_store::save(
        &root.join("parked"),
        &name,
        &json!({
        "request":request.id,"observation":observation["id"],"unconfirmed_observation":true,"reason":"status uncertainty remains observer-owned"}),
    )?;
    Ok(Some(
        json!({"state":"checkpoint_unconfirmed_observation","passed":null,"violations":[],
        "needs_user":[],"needs_thinker":[],"unproven":["phase-claim-review"],
        "observation":observation["id"],"automatic_reasks_stopped":true}),
    ))
}

fn observation(request: &Request, report: &Report, judgment: &Judgment) -> Value {
    let id = format!(
        "observation-{:x}",
        Sha256::digest(format!("{}:{}:claim_made", request.worker, request.id))
    );
    let claims = report
        .findings
        .iter()
        .filter(|f| matches!(f.topic, kah_observer::review_evidence::Topic::Claim))
        .collect::<Vec<_>>();
    json!({"id":id,"kind":"claim_observation","worker":request.worker,"requirement":"phase-claim-review",
        "name":"Unconfirmed native completion assertion","request":request.id,"head":request.head,
        "intent_revision":request.intent["revision"],"status":"unconfirmed","user_decision":false,
        "summary":"Status interpretation remains observer-owned after reconsideration; no user question and no gate verdict.",
        "next_steps":["Read existing repo/tags/STATUS/receipts and message evidence","Ask builder through delegate kah send for specific evidence, never authority","Run or propose an objective exact-SHA or targeted check","Retain any unresolved observation visibly and keep observing"],
        "claim_findings":claims,"judgment":judgment,"candidate_trigger":request.trigger,
        "scope":"This observation does not occupy the review slot; unrelated reviews continue",
        "at_ms":kah_observer::session::now_ms()})
}
