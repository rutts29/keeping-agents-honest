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
        crate::run_pane::invoke_at(&["agent", "prompt", "kah-reviewer", &text], Some(socket))
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
