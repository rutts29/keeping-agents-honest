//! A failed filesystem sample must not silently terminate observer supervision.
use super::super::state::{self, Store};
use crate::Result;
use serde_json::{Value, json};

pub(super) fn record(worker: &str, error: &dyn std::fmt::Display) -> Result<()> {
    let root = Store::directory(worker)?.join("liveness");
    kah_core::run_store::ensure_private_directory(&root)?;
    let error = error.to_string();
    let previous: Option<Value> = state::read(&root.join("watch-error.json"))?;
    if previous.as_ref().is_some_and(|v| {
        v["error"] == error && v["submitted"] == true && v["recovered_ms"].is_null()
    }) {
        return Ok(());
    }
    let text = format!(
        "[kah-builder] [kah-observer-watch-error] worker={worker}: {error}. Supervision continues; pending evidence is retained."
    );
    let sent = crate::relay_store::endpoint(worker).and_then(|endpoint| {
        let socket = endpoint
            .socket
            .as_deref()
            .ok_or("REVIEW_HERDR_REQUIRED stage=watch_error")?;
        crate::run_pane::invoke_at(
            &[
                "agent",
                "prompt",
                crate::reviewer_agent::reviewer_agent(),
                &text,
            ],
            Some(socket),
        )
    });
    crate::unattended_report::save(
        &root,
        "watch-error.json",
        &json!({
            "worker":worker,"error":error,"at_ms":kah_observer::session::now_ms(),
            "submitted":sent.is_ok(),"delivery_error":sent.err().map(|e|e.to_string())
        }),
    )?;
    crate::unattended_report::save(
        &root,
        "notice.json",
        &json!({
            "text":text,"state":"watch_error","worker":worker
        }),
    )?;
    eprintln!("kah observer supervision error: {error}");
    Ok(())
}

pub(super) fn recovered(worker: &str) -> Result<()> {
    let root = Store::directory(worker)?.join("liveness");
    if let Some(mut prior) = state::read::<Value>(&root.join("watch-error.json"))?
        && prior["recovered_ms"].is_null()
    {
        prior["recovered_ms"] = json!(kah_observer::session::now_ms());
        crate::unattended_report::save(&root, "watch-error.json", &prior)?;
    }
    Ok(())
}

#[cfg(test)]
#[path = "../../../tests/observer_unit/liveness_failure.rs"]
mod tests;

pub(super) fn notice_state(
    state: &super::super::state::State,
    p: &super::Progress,
) -> Result<Option<Value>> {
    if !p.sample.cause.contains("REVIEW_EXPANSION_PENDING") {
        return Ok(None);
    }
    let record: Option<Value> = super::super::state::read(
        &Store::directory(&state.worker.session)?.join("validation-current.json"),
    )?;
    Ok(Some(
        json!({"request":p.sample.request,"request_digest":record.as_ref().map(|v|&v["request_digest"]),
        "report_digest":record.as_ref().map(|v|&v["report_digest"]),"class":"failed_host_expansion"}),
    ))
}

pub(super) fn take_notice(worker: &str) -> Result<Option<Value>> {
    let path = Store::directory(worker)?.join("liveness/notice.json");
    let Some(value) = super::super::state::read::<Value>(&path)? else {
        return Ok(None);
    };
    std::fs::remove_file(path)?;
    Ok(Some(json!({"systemMessage":value["text"]})))
}

pub(super) fn already_notified(prior: Option<&Value>, request: &str, key: Option<&Value>) -> bool {
    key.is_some_and(|key| {
        prior.is_some_and(|v| {
            v["request"] == request && v["notice_state"] == *key && v["submitted"] == true
        })
    })
}
