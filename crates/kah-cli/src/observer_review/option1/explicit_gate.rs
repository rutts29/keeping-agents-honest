//! C5753c6: cumulative Option1 evidence is input, not the explicit gate decision.
use super::super::{
    capture, queue,
    state::{Purpose, Request, State, Store},
};
use crate::Result;
use kah_observer::{commit_check::Row, review_evidence::Document};
use serde_json::{Value, json};

pub fn required(trigger: &Value) -> bool {
    let original = trigger
        .get("option1")
        .map(|v| &v["original_trigger"])
        .unwrap_or(trigger);
    if original["review_class"] == "commit" {
        return false;
    }
    std::iter::once(original)
        .chain(original["events"].as_array().into_iter().flatten())
        .any(|event| {
            event["kind"] == "user_gate" && event["source"] == "host_cli"
                || event["kind"] == "native_phase_claim" && event["review_class"] == "gate"
        })
}
pub fn prepare(cumulative: &Request, mut captured: Request, rows: &[Row]) -> Result<Request> {
    if !(required(&cumulative.trigger) || required(&captured.trigger))
        || cumulative.worker != captured.worker
        || cumulative.head != captured.head
        || cumulative.intent["revision"] != captured.intent["revision"]
    {
        return Err("REVIEW_EXPLICIT_GATE_BINDING stage=checkpoint: cumulative input and typed decision must use the same worker, SHA and intent revision".into());
    }
    captured.purpose = Purpose::Checkpoint;
    captured
        .trigger
        .as_object_mut()
        .ok_or("REVIEW_EXPLICIT_GATE_TRIGGER stage=checkpoint")?
        .remove("option1");
    captured.trigger["decision_protocol_version"] = kah_observer::decision_units::VERSION.into();
    captured.trigger["option1_cumulative_input"] = json!({"request":cumulative.id,"head":cumulative.head,"intent_revision":cumulative.intent["revision"]});
    captured.documents.push(Document::new("option1-cumulative", "cumulative_review_input", &format!("kah:{}",cumulative.id),
        &serde_json::to_string(&json!({"request":cumulative,"rows":rows,"role":"Cumulative carry/fold/interaction evidence only; explicit gate requires the typed v12 verify-first decision."}))?));
    Ok(captured)
}

pub(super) fn carry(
    store: &Store,
    state: &mut State,
    turn: &str,
    trigger: Value,
    previous: &Value,
) -> Result<Value> {
    let id = previous["request"]
        .as_str()
        .ok_or("CHECKPOINT_CARRY_REQUEST stage=gate")?;
    let cumulative: Request = super::super::source_store::read(
        &store.directory.join("requests").join(format!("{id}.json")),
    )?;
    let rows: Vec<Row> = serde_json::from_value(previous["result"]["rows"].clone())?;
    let saved = store.directory.join("explicit-gate-inputs");
    let next = match super::super::state::read::<Request>(&saved.join(format!("{id}.json")))? {
        Some(next) => next,
        None => {
            let captured = capture::request(state, Purpose::Checkpoint, turn, trigger)?;
            let next = prepare(&cumulative, captured, &rows)?;
            save_snapshot(&saved, &cumulative, &next)?;
            next
        }
    };
    queue::enqueue_once_at(
        &store.directory,
        &format!("explicit-gate-carry:{id}"),
        &queue::Wake {
            purpose: Purpose::Checkpoint,
            turn: turn.into(),
            trigger: json!({"captured_snapshot":next}),
        },
    )?;
    state.checkpoint = Some(
        json!({"head":next.head,"intent_revision":next.intent["revision"],
        "request":next.id,"passed":null,"state":"verify_first_pending","carried_from":id}),
    );
    store.save(state)?;
    Ok(
        json!({"status":"option1_reviewed","head":next.head,"carried_from":id,
        "typed_continuation":next.id,"new_cumulative_reviews":0}),
    )
}
pub(super) fn enqueue(store: &Store, state: &State, request: &Request, rows: &[Row]) -> Result<()> {
    let root = store.directory.join("explicit-gate-inputs");
    let saved = root.join(format!("{}.json", request.id));
    let next = match super::super::state::read::<Request>(&saved)? {
        Some(next) => next,
        None => {
            let original = request.trigger["option1"]["original_trigger"].clone();
            let captured = capture::request(state, Purpose::Checkpoint, &request.turn, original)?;
            let next = prepare(request, captured, rows)?;
            save_snapshot(&root, request, &next)?;
            next
        }
    };
    queue::enqueue_once_at(
        &store.directory,
        &format!("explicit-gate:{}", request.id),
        &queue::Wake {
            purpose: Purpose::Checkpoint,
            turn: request.turn.clone(),
            trigger: json!({"captured_snapshot":next}),
        },
    )?;
    Ok(())
}
fn save_snapshot(root: &std::path::Path, cumulative: &Request, next: &Request) -> Result<()> {
    kah_core::run_store::ensure_private_directory(root)?;
    super::super::source_store::save(root, &format!("{}.json", cumulative.id), &json!(next))
}

#[cfg(test)]
#[path = "../../../tests/observer_unit/explicit_gate_storage.rs"]
mod storage_tests;
