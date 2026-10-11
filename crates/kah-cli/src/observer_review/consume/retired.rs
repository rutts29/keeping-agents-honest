//! Old tripwire work is a terminal gap, never a new judgment or a pass.
use super::super::state::{self, Purpose, State, Store};
use crate::Result;
use serde_json::{Value, json};

pub fn pending(worker: &str) -> Result<bool> {
    let root = Store::directory(worker)?;
    {
        let _queue = Store::delivery_at(&root.join("queue-receipts"), true)?;
        queued_at(&root)?;
    }
    let Some(state) = Store::read(worker)? else {
        return Ok(false);
    };
    if state
        .pending
        .as_ref()
        .is_none_or(|p| p.purpose != Purpose::Drift)
    {
        return Ok(false);
    }
    let store = Store::open_wait(worker)?;
    at(&store)
}
pub fn at(store: &Store) -> Result<bool> {
    let Some(mut state) = store.load()? else {
        return Ok(false);
    };
    let Some(pending) = state
        .pending
        .clone()
        .filter(|p| p.purpose == Purpose::Drift)
    else {
        return Ok(false);
    };
    if pending.id.is_empty()
        || !pending
            .id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-')
    {
        return Err("REVIEW_REQUEST_ID stage=retired_tripwire".into());
    }
    let request: Option<Value> = state::read(
        &store
            .directory
            .join("requests")
            .join(format!("{}.json", pending.id)),
    )?;
    let report_path = state
        .observer
        .workspace
        .join("outbox")
        .join(format!("submitted-{}.json", pending.id));
    let report: Option<Value> = state::read(&report_path)?;
    if report
        .as_ref()
        .is_some_and(|r| !r["decision_units"].is_null())
    {
        return Ok(false);
    }
    let result = json!({"request":pending.id,"head":pending.head,"status":"not_checked",
        "gap":{"code":"retired_tripwire","stage":"consume","reason":"The old tripwire lane is retired; commit checks cover commits from now on."},
        "assessment":{"state":"not_checked","passed":null},"real_jev_calls":0,
        "original_pending":pending,"original_request":request,"original_report":report,
        "original_report_path":report_path});
    archive(store, &mut state, &pending.id, &result)?;
    Ok(true)
}
pub fn queued_at(root: &std::path::Path) -> Result<usize> {
    let queued = root.join("queued");
    if !queued.exists() {
        return Ok(0);
    }
    let mut count = 0;
    let directory = root.join("retired-tripwire/queued");
    for entry in std::fs::read_dir(&queued)? {
        let path = entry?.path();
        if path.extension().is_none_or(|s| s != "json") {
            continue;
        }
        let Some(wake) = state::read::<Value>(&path)? else {
            continue;
        };
        if wake["purpose"] != "drift" {
            continue;
        }
        kah_core::run_store::ensure_private_directory(&directory)?;
        crate::unattended_report::save(
            &directory,
            &format!("gap-{}", path.file_name().unwrap().to_str().unwrap()),
            &json!({"status":"not_checked","gap":{"code":"retired_tripwire","stage":"queue"},"original":wake,"real_jev_calls":0}),
        )?;
        preserve_queued(&path, &directory.join(path.file_name().unwrap()))?;
        count += 1;
    }
    Ok(count)
}
fn preserve_queued(source: &std::path::Path, target: &std::path::Path) -> Result<()> {
    if target.exists() {
        if std::fs::read(source)? != std::fs::read(target)? {
            return Err("REVIEW_RETIRED_SOURCE_CHANGED stage=retired_tripwire".into());
        }
        std::fs::remove_file(source)?;
    } else {
        std::fs::rename(source, target)?;
    }
    Ok(())
}
fn archive(store: &Store, state: &mut State, request: &str, result: &Value) -> Result<()> {
    let directory = store.directory.join("retired-tripwire");
    kah_core::run_store::ensure_private_directory(&directory)?;
    crate::unattended_report::save(&directory, &format!("{request}.json"), result)?;
    state.latest = Some(result.clone());
    if state.pending.as_ref().is_some_and(|p| p.id == request) {
        state.pending = None;
    }
    store.save(state)?;
    super::super::source_store::finish_source_view(&state.observer.workspace, request)
}
