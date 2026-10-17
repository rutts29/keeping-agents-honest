//! An observer turn that ends without a report for the pending request must
//! not leave it silently pending (for example, after the transport drops and the
//! request remains queued). Retry it once in a fresh turn; if that turn
//! also ends without a report, retire it with a visible, typed gap. A request
//! that no native turn accepts by UNDELIVERED_MS is retired the same way.
use super::state::Store;
use crate::Result;
use serde_json::{Value, json};
use std::path::Path;

pub struct Gap {
    pub code: &'static str,
    pub reason: &'static str,
}
impl Gap {
    pub fn error(&self) -> String {
        format!(
            "{} stage=observer: {}; this request was NOT reviewed",
            self.code, self.reason
        )
    }
}
pub const NO_REPORT: Gap = Gap {
    code: "REVIEW_OBSERVER_NO_REPORT",
    reason: "two observer turns ended without a submitted evidence report",
};
pub const UNDELIVERED: Gap = Gap {
    code: "REVIEW_OBSERVER_UNDELIVERED",
    reason: "no native observer turn accepted the request within 10 minutes",
};
/// An undelivered request can wait while Herdr still shows the pane Working.
pub const UNDELIVERED_MS: u64 = 600_000;

/// Counted from the host's first redelivery, so a request that only waited
/// behind a live native turn is always attempted before it can be retired.
pub fn undelivered_expired(first_attempt_ms: Option<u64>, now_ms: u64, accepted: bool) -> bool {
    !accepted && first_attempt_ms.is_some_and(|at| now_ms.saturating_sub(at) >= UNDELIVERED_MS)
}

pub fn first_attempt_ms(directory: &Path, request: &str) -> Result<Option<u64>> {
    let log: Option<Value> = super::state::read(&directory.join("redelivery.json"))?;
    Ok(log.and_then(|l| l["requests"][request]["attempts"][0]["at_ms"].as_u64()))
}

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
pub fn not_reviewed(request: &str, head: &Value, gap: &Gap) -> Value {
    json!({"request":request,"head":head,"status":"not_reviewed",
        "gap":{"code":gap.code,"stage":"observer","reason":gap.reason},
        "assessment":{"state":"not_reviewed","passed":null}})
}

pub fn retire(worker: &str, request: &str, gap: &Gap) -> Result<()> {
    let store = Store::open_wait(worker)?;
    let Some(mut state) = store.load()? else {
        return Ok(());
    };
    let Some(pending) = state.pending.clone().filter(|p| p.id == request) else {
        return Ok(());
    };
    let result = not_reviewed(request, &json!(pending.head), gap);
    crate::unattended_report::save(
        &store.directory.join("results"),
        &format!("{request}.json"),
        &result,
    )?;
    crate::unattended_report::save(
        &store.directory,
        "gap.json",
        &json!({"error":gap.error(),"request":request,"at_ms":kah_observer::session::now_ms(),
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
