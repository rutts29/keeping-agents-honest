//! Retry accounting is telemetry, never a cap or a reason to discard a request.
use super::state::Store;
use crate::Result;
use serde_json::{Value, json};
mod correction;
pub(crate) mod expansion_retry;
mod parked;
mod reviewer;
pub use correction::*;
pub use reviewer::notify;

pub fn retryable(origin: super::failure::Origin, error: &str) -> bool {
    super::failure::correctable(origin, error)
        || origin == super::failure::Origin::Delivery
            && !error.starts_with("REVIEW_SESSION_")
            && !error.starts_with("REVIEW_HOOK_SCOPE")
}
pub fn gap_retryable(gap: &Value) -> bool {
    if gap["error"]
        .as_str()
        .is_some_and(|e| e.starts_with("REVIEW_EXPANSION_PENDING"))
    {
        return true;
    }
    let origin = serde_json::from_value(gap["origin"].clone()).unwrap_or_default();
    gap["error"]
        .as_str()
        .is_some_and(|error| retryable(origin, error))
}

pub fn prompt(
    home: &std::path::Path,
    store: &Store,
    state: &super::state::State,
    request: &super::state::Request,
    decision: &str,
) -> Result<String> {
    // Preserve the request and refresh helper/schema/instructions. A commit
    // packet may add a verified whole outside file, retaining its prior scope.
    super::workspace::prepare(home, state)?;
    let rejected: Option<Value> =
        super::state::read(&store.directory.join("validation-current.json"))?;
    let expansion_required = rejected.as_ref().is_some_and(|r| {
        r["request"] == request.id
            && r["error"]
                .as_str()
                .is_some_and(|e| e.starts_with("REVIEW_SOURCE_OUTSIDE_PACKET"))
    });
    // A context-selection wake already has complete immutable input sources.
    // Rebuilding the unrelated whole report packet held the review lock and
    // allocated GiB before sending the actual preparation prompt.
    if !expansion_required && super::preparation::pending(&request.worker, decision)? {
        return super::preparation::prompt(&request.worker);
    }
    let expanded = super::commit_packet::expand_rejected(store, state, request)?;
    if expansion_required && !expansion_changed(request, &expanded) {
        return Err("REVIEW_EXPANSION_PENDING stage=packet: rejected source was not added; host binding or source needs attention".into());
    }
    let report_prompt = super::workspace::publish(store, state, &expanded)?;
    if super::preparation::pending(&request.worker, decision)? {
        super::preparation::prompt(&request.worker)
    } else {
        Ok(report_prompt)
    }
}
pub fn expansion_changed(
    original: &super::state::Request,
    expanded: &super::state::Request,
) -> bool {
    original.documents.len() != expanded.documents.len()
        || original
            .documents
            .iter()
            .zip(&expanded.documents)
            .any(|(old, new)| old.id != new.id || old.digest != new.digest || old.kind != new.kind)
}

pub fn begin(store: &Store, request: &str, error: &str) -> Result<u64> {
    let mut log = load(store)?;
    let record = log["requests"]
        .as_object_mut()
        .unwrap()
        .entry(request)
        .or_insert_with(|| json!({"attempts":[],"consecutive_failures":0}));
    let failures = record["consecutive_failures"].as_u64().unwrap_or(0) + 1;
    record["consecutive_failures"] = failures.into();
    let attempts = record["attempts"].as_array_mut().unwrap();
    let number = attempts.len() as u64 + 1;
    attempts.push(json!({"number":number,"prior_error":error,"status":"requested","at_ms":kah_observer::session::now_ms()}));
    crate::unattended_report::save(&store.directory, "redelivery.json", &log)?;
    if failures >= 2 {
        crate::unattended_report::save(
            &store.directory,
            "redelivery-notice.json",
            &json!({"request":request,"failure":failures,"text":format!("kah observer request {request} has failed {failures} consecutive times: {error}. The review remains pending and retries continue on new events.")}),
        )?;
    }
    Ok(number)
}
pub fn outcome(store: &Store, request: &str, number: u64, error: Option<String>) -> Result<()> {
    let mut log = load(store)?;
    let attempts = log["requests"][request]["attempts"]
        .as_array_mut()
        .ok_or("REVIEW_REDELIVERY_ATTEMPT stage=telemetry")?;
    let attempt = attempts
        .iter_mut()
        .find(|a| a["number"] == number)
        .ok_or("REVIEW_REDELIVERY_ATTEMPT stage=telemetry")?;
    attempt["status"] = if error.is_some() {
        "delivery_failed"
    } else {
        "submitted_waiting_for_report"
    }
    .into();
    attempt["error"] = json!(error);
    crate::unattended_report::save(&store.directory, "redelivery.json", &log)
}
pub fn completed(store: &Store, request: &str) -> Result<()> {
    let mut log = load(store)?;
    if let Some(record) = log["requests"].get_mut(request) {
        record["consecutive_failures"] = 0.into();
        record["completed_ms"] = kah_observer::session::now_ms().into();
        crate::unattended_report::save(&store.directory, "redelivery.json", &log)?;
    }
    let notice = store.directory.join("redelivery-notice.json");
    if super::state::read::<Value>(&notice)?.is_some_and(|v| v["request"] == request) {
        std::fs::remove_file(notice)?;
    }
    Ok(())
}
fn load(store: &Store) -> Result<Value> {
    let saved = super::state::read::<Value>(&store.directory.join("redelivery.json"))?;
    Ok(match saved {
        Some(value) if value["requests"].is_object() => value,
        Some(value) => json!({"requests":{},"legacy":[value]}),
        None => json!({"requests":{}}),
    })
}
pub fn take(store: &Store) -> Result<Option<Value>> {
    let path = store.directory.join("redelivery-notice.json");
    let Some(notice) = super::state::read::<Value>(&path)? else {
        return Ok(None);
    };
    std::fs::remove_file(path)?;
    Ok(Some(json!({"systemMessage":notice["text"]})))
}
