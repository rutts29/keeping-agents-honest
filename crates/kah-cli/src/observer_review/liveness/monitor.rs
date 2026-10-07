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

pub fn waiting(worker: &str) -> Result<Option<Processing>> {
    Store::read(worker)?
        .and_then(|s| s.pending.map(|p| processing(worker, &p.id)))
        .transpose()
}
pub fn run(worker: &str) -> Result<()> {
    super::super::runtime::require_active(worker)?;
    if super::super::service::blocked(worker)? {
        return Ok(());
    }
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
    let acknowledged = super::awaiting_host(&state)?;
    if acknowledged {
        super::super::hook::spawn("__review-consume", worker)?;
    }
    let sample = sample(&state)?;
    let prior: Option<Progress> = super::super::state::read(&root.join("progress.json"))?;
    if prior.as_ref().is_some_and(|p| {
        (p.stalled || validation_blocked(&p.sample)) && p.sample.request != sample.request
    }) {
        recovered(&root, worker)?;
    }
    let progress = transition(prior.as_ref(), &sample);
    crate::unattended_report::save(&root, "progress.json", &serde_json::to_value(&progress)?)?;
    if progress.changed {
        event(&root, worker, &progress)?;
    }
    if validation_blocked(&sample) {
        return validation_notice(&state, &progress);
    }
    if !progress.stalled {
        return Ok(());
    }
    if sample.native_active {
        // Advisory only. A human decides whether to interrupt a Working pane.
        return notify(&state, &progress);
    }
    let error = format!(
        "REVIEW_OBSERVER_STALLED stage=liveness: request {}: {}",
        pending.id, sample.cause
    );
    crate::unattended_report::save(
        &Store::directory(worker)?,
        "gap.json",
        &json!({"error":error,"at_ms":sample.now_ms,"origin":super::super::failure::Origin::Delivery}),
    )?;
    notify(&state, &progress)?;
    super::super::hook::spawn("__review-retry", worker)
}
pub fn watch(worker: &str) -> Result<()> {
    loop {
        if let Err(error) = run(worker) {
            if error.to_string().starts_with("REVIEW_PAUSED") {
                return Ok(());
            }
            super::failure::record(worker, error.as_ref())?;
        } else {
            super::failure::recovered(worker)?;
        }
        // Keep the cheap host watcher across review handoffs; otherwise a
        // publish/delivery crash while the worker is idle loses its watcher.
        match Store::read(worker) {
            Ok(state) if state.as_ref().is_none_or(|s| s.stopped) => return Ok(()),
            Err(error) => super::failure::record(worker, error.as_ref())?,
            _ => (),
        }
        // Filesystem sampling, no model polling or extra native turns.
        std::thread::sleep(std::time::Duration::from_secs(15));
    }
}
pub fn start(worker: &str) -> Result<()> {
    use std::os::unix::process::CommandExt;
    use std::process::{Command, Stdio};
    let root = Store::directory(worker)?;
    let lock = fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .mode(0o600)
        .open(root.join("job-liveness-watch.lock"))?;
    match lock.try_lock() {
        Ok(()) => (),
        Err(fs::TryLockError::WouldBlock) => return Ok(()),
        Err(e) => return Err(e.into()),
    }
    let log = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .mode(0o600)
        .open(root.join("job-liveness-watch.log"))?;
    let home = PathBuf::from(std::env::var_os("HOME").ok_or("REVIEW_HOME stage=liveness")?);
    Command::new(home.join(".local/bin/kah"))
        .args(["__review-liveness-watch", worker])
        .stdin(Stdio::from(lock))
        .stdout(Stdio::null())
        .stderr(Stdio::from(log))
        .process_group(0)
        .spawn()?;
    Ok(())
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
    // Working can belong to the previous review's closing native turn. It is
    // not record progress, but still must never be automatically interrupted
    // or re-prompted while a later request waits for that pane.
    let native_active = pane_working(state).unwrap_or(false);
    let controller_active = processing_active(&root, &pending.id)? || super::awaiting_host(state)?;
    let outbox = state.observer.workspace.join("outbox");
    let rejected = if super::super::preparation::pending(
        &state.worker.session,
        &super::super::preparation::active_decision(&state.worker.session, &pending.id)?,
    )? {
        super::selection_rejection::current(&root, &outbox, &state.worker.session, &pending.id)?
    } else {
        super::rejection::current(&root, &outbox, &state.worker.session, &pending.id)?
    };
    let host_error = host_failure(&root, &pending.id)?;
    let cause = if let Some(reason) = rejected {
        reason
    } else if let Some(error) = host_error {
        format!("VALIDATION BLOCKED: {error}")
    } else if Store::open(&state.worker.session).is_err() {
        "review lock busy".into()
    } else if state.observer.pane.is_none() {
        "observer pane absent".into()
    } else {
        "no current-request native activity or report growth".into()
    };
    Ok(Sample {
        request: pending.id.clone(),
        now_ms: kah_observer::session::now_ms(),
        started_ms: pending.created_ms,
        native_active,
        controller_active,
        transcript_version: state.observer.transcript.as_deref().and_then(version),
        report_version: report_version(&outbox, &pending.id, &report),
        cause,
    })
}
fn host_failure(root: &Path, request: &str) -> Result<Option<String>> {
    let gap: Option<Value> = super::super::state::read(&root.join("gap.json"))?;
    Ok(gap
        .filter(|g| {
            g["request"].as_str().is_none_or(|id| id == request)
                && (super::super::failure::correctable(
                    serde_json::from_value(g["origin"].clone()).unwrap_or_default(),
                    g["error"].as_str().unwrap_or(""),
                ) || !super::super::redelivery::gap_retryable(g))
        })
        .and_then(|g| g["error"].as_str().map(str::to_owned)))
}
fn pane_working(state: &State) -> Result<bool> {
    let (socket, _) = super::super::runtime::transport(state)?;
    let Some(pane) = state.observer.pane.as_deref() else {
        return Ok(false);
    };
    let view = crate::relay_transport::herdr(socket, &["pane", "get", pane])?;
    Ok(view["result"]["pane"]["agent_status"] == "working")
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
fn report_version(outbox: &Path, request: &str, submitted: &Path) -> Option<(u64, u64)> {
    [
        submitted.to_path_buf(),
        outbox.join(format!("{request}.json")),
        outbox.join(format!("rejected-{request}.json")),
    ]
    .iter()
    .filter_map(|path| version(path))
    .reduce(|(size, time), (next_size, next_time)| {
        (size.saturating_add(next_size), time.max(next_time))
    })
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
    let state = if validation_blocked(&p.sample) {
        "validation_blocked"
    } else if p.stalled && p.sample.native_active {
        "possibly_hung"
    } else if p.stalled {
        "stalled"
    } else {
        "recovered"
    };
    let text = format!(
        "kah observer {state}: worker {worker}, request {}: {}; last native record time {:?}. A Working pane is never automatically interrupted.",
        p.sample.request,
        p.sample.cause,
        p.sample.transcript_version.map(|(_, time)| time)
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
        && (p.stalled || validation_blocked(&p.sample))
    {
        p.stalled = false;
        p.changed = true;
        p.sample.now_ms = kah_observer::session::now_ms();
        p.sample.cause = "review request completed".into();
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
            && v["cause"] == p.sample.cause
            && (validation_blocked(&p.sample) || v["last_progress_ms"] == p.last_progress_ms)
            && v["submitted"] == true
    }) {
        return Ok(());
    }
    let label = if validation_blocked(&p.sample) {
        "validation-blocked"
    } else if p.sample.native_active {
        "possibly-hung"
    } else {
        "stalled"
    };
    let action = if validation_blocked(&p.sample) {
        "The exact failed report still fails host validation; pending review retained for correction."
    } else if p.sample.native_active {
        "Native Working has no new records; human decision required, no automatic interruption or prompt."
    } else {
        "Pending review retained; visible redelivery attempted."
    };
    let text = format!(
        "[kah-builder] [kah-observer-{label}] worker={} request={} {}; last native record time {:?}. {action}",
        state.worker.session,
        p.sample.request,
        p.sample.cause,
        p.sample.transcript_version.map(|(_, time)| time)
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
        &json!({"request":p.sample.request,"cause":p.sample.cause,"last_progress_ms":p.last_progress_ms,
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

fn validation_notice(state: &State, progress: &Progress) -> Result<()> {
    let root = Store::directory(&state.worker.session)?;
    let checked: Option<Value> = super::super::state::read(&root.join("validation-current.json"))?;
    let current: Option<Value> = super::super::state::read(&root.join("gap.json"))?;
    let mut gap=current.filter(|v|v["request"]==progress.sample.request && v["error"].as_str()==progress.sample.cause.strip_prefix("VALIDATION BLOCKED: ")).or_else(||checked.filter(|v|v["request"]==progress.sample.request && v["error"].as_str()==progress.sample.cause.strip_prefix("VALIDATION BLOCKED: ")))
            .unwrap_or_else(||json!({"error":progress.sample.cause.strip_prefix("VALIDATION BLOCKED: "),"origin":super::super::failure::Origin::Host}));
    gap["status"] = "validation_blocked".into();
    gap["at_ms"] = progress.sample.now_ms.into();
    crate::unattended_report::save(&root, "gap.json", &gap)?;
    notify(state, progress)?;
    if super::super::redelivery::gap_retryable(&gap)
        && !progress.sample.native_active
        && !progress.sample.controller_active
    {
        super::super::hook::spawn("__review-retry", &state.worker.session)?;
    }

    Ok(())
}

#[cfg(test)]
#[path = "../../../tests/observer_unit/liveness_post_ack_match.rs"]
mod correction_tests;
