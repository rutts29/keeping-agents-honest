//! User/reviewer no-loop rule: one thinker reconsideration, then one question.
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
    if !rethink && previous.as_ref().is_none_or(|p| p["question"].is_null()) {
        return Ok(None);
    }
    let question = question(request, report, judgment);
    let directory = root.join("questions");
    kah_core::run_store::ensure_private_directory(&directory)?;
    let _lock = Store::delivery_at(&directory, true)?;
    let qname = format!("{}.json", question["id"].as_str().unwrap());
    if !directory.join(&qname).exists() {
        source_store::save(&directory, &qname, &question)?;
    }
    let mut record = record;
    record["question"] = question["id"].clone();
    source_store::save(&history, &name, &record)?;
    kah_core::run_store::ensure_private_directory(&root.join("parked"))?;
    source_store::save(
        &root.join("parked"),
        &name,
        &json!({
        "request":request.id,"question":question["id"],"reason":"claim uncertainty after thinker reconsideration"}),
    )?;
    Ok(Some(
        json!({"state":"checkpoint_needs_user","passed":null,"violations":[],
        "needs_user":["phase-claim-review"],"needs_thinker":[],"unproven":["phase-claim-review"],
        "question":question["id"],"automatic_reasks_stopped":true}),
    ))
}

fn question(request: &Request, report: &Report, judgment: &Judgment) -> Value {
    let id = format!(
        "question-{:x}",
        Sha256::digest(format!("{}:{}:claim_made", request.worker, request.id))
    );
    let claims = report
        .findings
        .iter()
        .filter(|f| matches!(f.topic, kah_observer::review_evidence::Topic::Claim))
        .collect::<Vec<_>>();
    json!({"id":id,"kind":"checkpoint_claim","worker":request.worker,"requirement":"phase-claim-review",
        "name":"Whether the native message asserted phase completion","request":request.id,"head":request.head,
        "intent_revision":request.intent["revision"],"reason":"Claim uncertainty persisted after one thinker reconsideration",
        "summary":"The thinker supplied candidate assertion evidence; Jev remained unsure after reconsideration. Automatic re-asks for this request stop.",
        "question":"Did the candidate builder message assert phase completion, or was it ordinary progress? Does existing user authority already resolve this?",
        "options":["Completion asserted","Ordinary progress","Existing decision resolves it"],
        "claim_findings":claims,"judgment":judgment,"candidate_trigger":request.trigger,
        "ledger_review":{"reasoning":"Check the cited native message and existing decisions; no new authority is inferred."},
        "authority_at_question":crate::observer_review::decisions::active_items(&request.intent),
        "goal_at_question":request.intent["goal"],"status":"pending","delivery":"queued","question_origin":"controller",
        "scope":"Only this decision waits; unrelated reviews continue","at_ms":kah_observer::session::now_ms()})
}
