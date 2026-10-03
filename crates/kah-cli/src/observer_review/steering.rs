use super::state::Request;
use crate::Result;
use kah_core::run_store::RunStore;
use kah_observer::{
    engine,
    intake::{Decision, Input},
    judge::{self, Judgment},
    session::Store,
};
use serde_json::{Value, json};
use std::{path::Path, time::Duration};

pub fn pending(worker: &str) -> Result<Option<Value>> {
    let store = Store::open(&RunStore::default_state_root()?, worker, Duration::ZERO)?;
    Ok(store
        .sources()?
        .into_iter()
        .find(|i| {
            !i.dismissed && !i.promoted && matches!(i.decision, Some(Decision::Verification { .. }))
        })
        .map(|i| json!({"uncertain_user_input":i})))
}
pub fn judge(home: &Path, request: &Request, actions: &[Value]) -> Result<(Judgment, Value)> {
    let source: Input = serde_json::from_value(request.trigger["uncertain_user_input"].clone())?;
    let items = request.intent["items"]
        .as_array()
        .map(|items| {
            items
                .iter()
                .filter_map(|i| {
                    Some((i["id"].as_str()?.to_owned(), i["text"].as_str()?.to_owned()))
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let mut input = judge::questions::steering_state(
        request.intent["goal"].as_str(),
        goal_item(request).as_deref(),
        &items,
        &source.text,
        "",
    );
    input["full_context_verified_evidence"] = json!(actions);
    crate::privacy::scrub(&mut input);
    let binding = Store::read(&RunStore::default_state_root()?, &request.worker)?
        .ok_or("REVIEW_WORKER_STATE stage=steering")?
        .binding;
    let judgment = judge::routing::evaluate(
        home,
        binding.judge,
        &binding.jev_model,
        &input,
        &judge::questions::steering(&items),
    )?;
    Ok((judgment, input))
}
fn goal_item(request: &Request) -> Option<String> {
    let ledger: Value =
        serde_json::from_str(&request.documents.iter().find(|d| d.id == "ledger")?.text).ok()?;
    let sources = ledger["goal"]["source_refs"].as_array()?;
    ["preserve", "frontier", "validation"]
        .iter()
        .filter_map(|k| ledger[*k].as_array())
        .flatten()
        .find(|i| {
            i["introduced_by"]
                .as_array()
                .is_some_and(|refs| refs.iter().any(|r| sources.contains(r)))
        })
        .and_then(|i| i["id"].as_str().map(str::to_owned))
}
/// Preserve any intervening explicit user review. The next native event applies
/// the verified decision through the existing verbatim ledger machinery.
pub fn apply(request: &Request, judgment: &Judgment) -> Result<&'static str> {
    let source: Input = serde_json::from_value(request.trigger["uncertain_user_input"].clone())?;
    let mut store = Store::open(
        &RunStore::default_state_root()?,
        &request.worker,
        Duration::from_secs(2),
    )?;
    let Some(current) = store
        .sources()?
        .into_iter()
        .find(|i| i.key() == source.key())
    else {
        return Err("REVIEW_STEERING_SOURCE stage=steering".into());
    };
    if current.dismissed
        || current.promoted
        || !matches!(current.decision, Some(Decision::Verification { .. }))
    {
        return Ok("explicit_user_review_preserved");
    }
    if current.text != source.text || current.source != source.source {
        return Err("REVIEW_STEERING_SOURCE_CHANGED stage=steering".into());
    }
    let state = store.load()?.ok_or("REVIEW_WORKER_STATE stage=steering")?;
    let current = serde_json::to_value(engine::projection(&state)?)?;
    let previous: Value = serde_json::from_str(
        &request
            .documents
            .iter()
            .find(|d| d.id == "ledger")
            .ok_or("REVIEW_LEDGER_MISSING stage=steering")?
            .text,
    )?;
    if current["revision"] != previous["revision"] {
        return Ok("intent_changed_recheck_next_turn");
    }
    store.source_decision(
        source.key(),
        &Decision::Jev {
            judgment: Box::new(judgment.clone()),
        },
    )?;
    store.append(&json!({"kind":"judgment","purpose":"verified_steering","source":"observer","question_set":judge::questions::VERSION,"judgment":judgment,"request":request.id}))?;
    Ok("verified_for_next_native_event")
}
