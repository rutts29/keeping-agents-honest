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
pub fn covered(worker: &str, trigger: &Value) -> Result<bool> {
    let source: Input = serde_json::from_value(trigger["uncertain_user_input"].clone())?;
    let store = match Store::open(&RunStore::default_state_root()?, worker, Duration::ZERO) {
        Ok(store) => store,
        Err(error) if error.starts_with("OBSERVER_BUSY") => return Ok(false),
        Err(error) => return Err(error.into()),
    };
    Ok(store.sources()?.iter().any(|current| {
        current.key() == source.key()
            && current.text == source.text
            && current.source == source.source
            && (current.dismissed
                || current.promoted
                || matches!(
                    current.decision,
                    Some(Decision::Jev { .. } | Decision::Validated { .. })
                ))
    }))
}
pub fn judge(home: &Path, request: &Request, actions: &[Value]) -> Result<(Judgment, Value)> {
    let source: Input = serde_json::from_value(request.trigger["uncertain_user_input"].clone())?;
    let store = Store::open(
        &RunStore::default_state_root()?,
        &request.worker,
        Duration::from_secs(2),
    )?;
    let prior = kah_observer::chronology::stored_context(&store, &source)?;
    drop(store);
    let items = prior["items"]
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
        prior["goal"].as_str(),
        prior["goal_item_id"].as_str(),
        &items,
        &source.text,
        "",
    );
    input["full_context_verified_evidence"] = json!(actions);
    crate::privacy::scrub(&mut input);
    if super::preparation::pending(&request.worker, &request.id)? {
        super::preparation::accept(&request.worker, &request.id)?;
    }
    let (judgment, input) =
        super::preparation::evaluate(home, request, &input, &judge::questions::steering(&items))?;
    super::steering_conflict::check(home, request)?;
    Ok((judgment, input))
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
