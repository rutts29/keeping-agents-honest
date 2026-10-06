use super::*;
use crate::{
    Result,
    observer_review::state::{State, Store},
};
use serde_json::{Value, json};
use std::os::unix::fs::OpenOptionsExt;
use std::{
    fs,
    path::{Path, PathBuf},
    time::UNIX_EPOCH,
};

pub fn run(worker: &str) -> Result<()> {
    super::super::runtime::require_active(worker)?;
    let root = Store::directory(worker)?.join("liveness");
    let _lock = match Store::delivery_at(&root, false) {
        Ok(lock) => lock,
        Err(e) if e.to_string().starts_with("REVIEW_BUSY") => return Ok(()),
        Err(e) => return Err(e),
    };
    let Some(state) = Store::read(worker)? else {
        return Ok(());
    };
    let Some(pending) = &state.pending else {
        return recovered(&root, worker);
    };
    let sample = sample(&state)?;
    let prior: Option<Progress> = super::super::state::read(&root.join("progress.json"))?;
    if prior
        .as_ref()
        .is_some_and(|p| p.stalled && p.sample.request != sample.request)
    {
        recovered(&root, worker)?;
    }
    let progress = transition(prior.as_ref(), &sample);
    crate::unattended_report::save(&root, "progress.json", &serde_json::to_value(&progress)?)?;
    if progress.changed {
        event(&root, worker, &progress)?;
    }
    if !progress.stalled {
        return Ok(());
    }
    let error = format!(
        "REVIEW_OBSERVER_STALLED stage=liveness: request {}: {}",
        pending.id, sample.cause
    );
    crate::unattended_report::save(
        &Store::directory(worker)?,
        "gap.json",
        &json!({"error":error,"at_ms":sample.now_ms}),
    )?;
    notify(&state, &progress)?;
    super::super::hook::spawn("__review-retry", worker)
}

fn sample(state: &State) -> Result<Sample> {
    let pending = state
        .pending
        .as_ref()
        .ok_or("REVIEW_PENDING_MISSING stage=liveness")?;
    let root = Store::directory(&state.worker.session)?;
    let report = state
        .observer
        .workspace
        .join("outbox")
        .join(format!("submitted-{}.json", pending.id));
    let native_active = super::super::delivery::in_progress(state).unwrap_or(false);
    let controller_active = processing_active(&root, &pending.id)?;
    let cause = if Store::open(&state.worker.session).is_err() {
        "review lock busy"
    } else if state.observer.pane.is_none() {
        "observer pane absent"
    } else {
        "no current-request native activity or report growth"
    };
    Ok(Sample {
        request: pending.id.clone(),
        now_ms: kah_observer::session::now_ms(),
        started_ms: pending.created_ms,
        native_active,
        controller_active,
        transcript_version: state.observer.transcript.as_deref().and_then(version),
        report_version: version(&report),
        cause: cause.into(),
    })
}
fn version(path: &Path) -> Option<(u64, u64)> {
    let meta = fs::metadata(path).ok()?;
    Some((
        meta.len(),
        meta.modified()
            .ok()?
            .duration_since(UNIX_EPOCH)
            .ok()?
            .as_millis() as u64,
    ))
}
fn active(path: &Path, request: &str) -> Result<bool> {
    let marker: Option<Value> = super::super::state::read(path)?;
    Ok(marker.is_some_and(|m| {
        m["request"] == request
            && m["pid"]
                .as_u64()
                .and_then(|p| kah_observer::session::process_stamp(p as u32))
                .is_some_and(|stamp| m["stamp"] == stamp)
    }))
}
fn processing_active(root: &Path, request: &str) -> Result<bool> {
    if active(&root.join("processing.json"), request)? {
        return Ok(true);
    }
    let directory = root.join("processing");
    if !directory.exists() {
        return Ok(false);
    }
    for entry in fs::read_dir(directory)? {
        let path = entry?.path();
        if path.extension().is_some_and(|e| e == "json") && active(&path, request)? {
            return Ok(true);
        }
    }
    Ok(false)
}
pub struct Processing(PathBuf);
impl Drop for Processing {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}
pub fn processing(worker: &str, request: &str) -> Result<Processing> {
    let root = Store::directory(worker)?.join("processing");
    kah_core::run_store::ensure_private_directory(&root)?;
    let filename = format!("{}.json", crate::relay_store::nonce());
    let pid = std::process::id();
    crate::unattended_report::save(
        &root,
        &filename,
        &json!({"request":request,"pid":pid,
        "stamp":kah_observer::session::process_stamp(pid)}),
    )?;
    Ok(Processing(root.join(filename)))
}
fn event(root: &Path, worker: &str, p: &Progress) -> Result<()> {
    let state = if p.stalled { "stalled" } else { "recovered" };
    let text = format!(
        "kah observer {state}: worker {worker}, request {}: {}",
        p.sample.request, p.sample.cause
    );
    crate::unattended_report::save(
        root,
        "notice.json",
        &json!({"text":text,"request":p.sample.request,"state":state}),
    )?;
    let mut file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .mode(0o600)
        .open(root.join("events.jsonl"))?;
    use std::io::Write;
    writeln!(
        file,
        "{}",
        json!({"worker":worker,"state":state,"progress":p})
    )?;
    file.sync_all()?;
    Ok(())
}
fn recovered(root: &Path, worker: &str) -> Result<()> {
    if let Some(mut p) = super::super::state::read::<Progress>(&root.join("progress.json"))?
        && p.stalled
    {
        p.stalled = false;
        p.changed = true;
        p.sample.now_ms = kah_observer::session::now_ms();
        event(root, worker, &p)?;
        crate::unattended_report::save(root, "progress.json", &serde_json::to_value(p)?)?;
    }
    Ok(())
}
fn notify(state: &State, p: &Progress) -> Result<()> {
    let root = Store::directory(&state.worker.session)?.join("liveness");
    let prior: Option<Value> = super::super::state::read(&root.join("reviewer.json"))?;
    if prior.as_ref().is_some_and(|v| {
        v["request"] == p.sample.request
            && v["last_progress_ms"] == p.last_progress_ms
            && v["submitted"] == true
    }) {
        return Ok(());
    }
    let text = format!(
        "[kah-builder] [kah-observer-stalled] worker={} request={} {}. Pending review retained; visible redelivery attempted.",
        state.worker.session, p.sample.request, p.sample.cause
    );
    let sent = state
        .worker
        .socket
        .as_deref()
        .ok_or_else(|| "REVIEW_HERDR_REQUIRED stage=liveness".into())
        .and_then(|socket| {
            crate::run_pane::invoke_at(&["agent", "prompt", "kah-reviewer", &text], Some(socket))
        });
    crate::unattended_report::save(
        &root,
        "reviewer.json",
        &json!({"request":p.sample.request,"last_progress_ms":p.last_progress_ms,
        "submitted":sent.is_ok(),"error":sent.err().map(|e|e.to_string())}),
    )?;
    Ok(())
}
pub fn notice(worker: &str) -> Result<Option<Value>> {
    let path = Store::directory(worker)?.join("liveness/notice.json");
    let Some(value) = super::super::state::read::<Value>(&path)? else {
        return Ok(None);
    };
    fs::remove_file(path)?;
    Ok(Some(json!({"systemMessage":value["text"]})))
}
