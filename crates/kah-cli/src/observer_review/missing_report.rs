//! An observer turn that ends without a report for the pending request must
//! not leave it silently pending (for example, after the transport drops and the
//! request remains queued). Retry it once in a fresh turn; if that turn
//! also ends without a report, retire it with a visible, typed gap.
use super::state::Store;
use crate::Result;
use serde_json::{Value, json};
use std::path::Path;

pub const NO_REPORT: &str = "REVIEW_OBSERVER_NO_REPORT stage=observer: two observer turns ended without a submitted evidence report; this request was NOT reviewed";

/// What the Stop hook must do for a pending request without a report.
#[derive(Debug, PartialEq, Eq)]
pub enum Next {
    Retry,
    Retire,
}

pub fn next(directory: &Path, request: &str) -> Result<Next> {
    let marker = marker(directory, request);
    if marker.exists() {
        return Ok(Next::Retire);
    }
    kah_core::run_store::ensure_private_directory(marker.parent().unwrap())?;
    crate::unattended_report::save(
        marker.parent().unwrap(),
        marker.file_name().unwrap().to_str().unwrap(),
        &json!({"request":request,"first_missing_ms":kah_observer::session::now_ms()}),
    )?;
    Ok(Next::Retry)
}

fn marker(directory: &Path, request: &str) -> std::path::PathBuf {
    directory
        .join("missing-report")
        .join(format!("{request}.json"))
}

/// The retired result: never a pass, and visible in results, gap.json and the
/// session journal.
pub fn not_reviewed(request: &str, head: &Value) -> Value {
    json!({"request":request,"head":head,"status":"not_reviewed",
        "gap":{"code":"REVIEW_OBSERVER_NO_REPORT","stage":"observer",
            "reason":"two observer turns ended without a submitted evidence report"},
        "assessment":{"state":"not_reviewed","passed":null}})
}

pub fn retire(worker: &str, request: &str) -> Result<()> {
    let store = Store::open_wait(worker)?;
    let Some(mut state) = store.load()? else {
        return Ok(());
    };
    let Some(pending) = state.pending.clone().filter(|p| p.id == request) else {
        return Ok(());
    };
    let result = not_reviewed(request, &json!(pending.head));
    crate::unattended_report::save(
        &store.directory.join("results"),
        &format!("{request}.json"),
        &result,
    )?;
    crate::unattended_report::save(
        &store.directory,
        "gap.json",
        &json!({"error":NO_REPORT,"request":request,"at_ms":kah_observer::session::now_ms(),
            "origin":super::failure::Origin::Host}),
    )?;
    state.latest = Some(result.clone());
    state.pending = None;
    store.save(&state)?;
    drop(store);
    super::timing::now(worker, request, "not_reviewed")?;
    journal(worker, &result)?;
    super::queue::kick(worker)
}

fn journal(worker: &str, result: &Value) -> Result<()> {
    let base = kah_core::run_store::RunStore::default_state_root()?;
    let mut session =
        kah_observer::session::Store::open(&base, worker, std::time::Duration::from_secs(10))?;
    session.append(
        &json!({"kind":"gap","source":"review_observer","gap":result["gap"],
        "request":result["request"]}),
    )?;
    Ok(())
}

#[cfg(test)]
#[path = "../../tests/observer_unit/missing_report.rs"]
mod tests;
