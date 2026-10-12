//! Heavy events share one verdict for the same code and authoritative intent.
use super::super::state::{Request, State, Store};
use crate::Result;
use serde_json::{Value, json};

pub fn previous(state: &State, head: &str, revision: u64) -> Option<Value> {
    let checkpoint = state.checkpoint.as_ref()?;
    if checkpoint["head"] != head
        || checkpoint["intent_revision"] != revision
        || checkpoint["state"] == "verify_first_pending"
        || !(checkpoint["passed"].is_boolean()
            || checkpoint["terminal"] == true
            || matches!(
                checkpoint["state"].as_str(),
                Some(
                    "checkpoint_needs_user" | "checkpoint_needs_thinker" | "checkpoint_unconfirmed"
                )
            ))
    {
        return None;
    }
    Some(checkpoint.clone())
}

pub fn empty(store: &Store, state: &mut State, request: &Request) -> Result<bool> {
    let input = &request.trigger["option1"]["input"];
    let Some(head) = request.head.as_deref() else {
        return Ok(false);
    };
    if input["code_changed"] != false
        || input["baseline"] != head
        || !input["files"].as_object().is_some_and(|f| f.is_empty())
        || super::explicit_gate::required(&request.trigger)
    {
        return Ok(false);
    }
    let revision = request.intent["revision"]
        .as_u64()
        .ok_or("CHECKPOINT_CARRY_REVISION stage=consume")?;
    let Some(checkpoint) = previous(state, head, revision) else {
        return Ok(false);
    };
    let trigger = &request.trigger["option1"]["original_trigger"];
    let result = json!({"request":request.id,"head":head,"state":"empty_range_carried",
        "carried_from":checkpoint["request"],"passed":checkpoint["passed"],"verdict":checkpoint,
        "trigger":trigger,"new_native_decisions":0,"new_jev_decisions":0});
    retain(store, &result)?;
    bind_completion(state, trigger);
    state.latest = Some(result);
    Ok(true)
}

pub fn admit(
    store: &Store,
    state: &mut State,
    head: &str,
    revision: u64,
    turn: &str,
    trigger: &Value,
) -> Result<Option<Value>> {
    if trigger["review_class"] == "commit" || state.pending.is_some() {
        return Ok(None);
    }
    let Some(checkpoint) = previous(state, head, revision) else {
        return Ok(None);
    };
    // Reuse the existing cumulative record and open only the typed gate step.
    if super::explicit_gate::required(trigger) && checkpoint["result"]["rows"].is_array() {
        return super::explicit_gate::carry(store, state, turn, trigger.clone(), &checkpoint)
            .map(Some);
    }
    let result = json!({"status":"option1_reviewed","head":head,"carried_from":checkpoint["request"],
        "intent_revision":revision,"passed":checkpoint["passed"],"trigger":trigger,
        "verdict":checkpoint,"new_native_decisions":0,"new_jev_decisions":0});
    retain(store, &result)?;
    bind_completion(state, trigger);
    state.latest = Some(result.clone());
    store.save(state)?;
    Ok(Some(result))
}

fn bind_completion(state: &mut State, trigger: &Value) {
    let event = std::iter::once(trigger)
        .chain(trigger["events"].as_array().into_iter().flatten())
        .find(|e| e["kind"] == "run_completion");
    if let (Some(checkpoint), Some(event)) = (&mut state.checkpoint, event)
        && checkpoint["completion_key"].is_null()
    {
        checkpoint["completion_key"] = event["completion_key"].clone();
    }
}

fn retain(store: &Store, result: &Value) -> Result<()> {
    use sha2::{Digest, Sha256};
    let root = store.directory.join("checkpoint-carries");
    kah_core::run_store::ensure_private_directory(&root)?;
    let id = format!("{:x}.json", Sha256::digest(serde_json::to_vec(result)?));
    super::super::source_store::save(&root, &id, result)
}
