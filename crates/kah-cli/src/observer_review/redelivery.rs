//! Retry accounting is telemetry, never a cap or a reason to discard a request.
use super::state::Store;
use crate::Result;
use serde_json::{Value, json};

pub fn prompt(
    home: &std::path::Path,
    store: &Store,
    state: &super::state::State,
    request: &super::state::Request,
    decision: &str,
) -> Result<String> {
    // The sources/request stay identical; only helper/schema/instructions are
    // refreshed so an in-flight native session can correct an older report.
    super::workspace::prepare(home, state)?;
    let report_prompt = super::workspace::publish(store, state, request)?;
    if super::preparation::pending(&request.worker, decision)? {
        super::preparation::prompt(&request.worker)
    } else {
        Ok(report_prompt)
    }
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
