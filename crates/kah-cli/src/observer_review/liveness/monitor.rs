use super::*;
#[path = "host_gap.rs"]
mod host_gap;
use crate::{
    Result,
    observer_review::state::{State, Store},
};
use host_gap::host_failure;
use serde_json::{Value, json};
use std::os::unix::fs::OpenOptionsExt;
use std::{
    fs,
    path::{Path, PathBuf},
    time::UNIX_EPOCH,
};

pub fn run(worker: &str) -> Result<()> {
    super::super::provider_block::observe(worker)?;
    super::super::runtime::require_active(worker)?;
    if super::super::service::blocked(worker)? {
        return Ok(());
    }
    super::super::native_receipt::ensure(worker)?;
    super::super::provider_block::observe(worker)?;
    super::super::runtime::require_active(worker)?;
    sample_bound(worker)
}
fn sample_bound(worker: &str) -> Result<()> {
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
    if service_outage(&sample) {
        return notify(&state, &progress);
    }
    if host_expansion(&sample) {
        return super::super::hook::spawn("__review-retry", worker);
    }
    if validation_blocked(&sample) {
        return validation_notice(&state, &progress);
    }
    if !progress.stalled {
        return Ok(());
    }
    if super::super::delivery::retry_held(&state, sample.native_active) || sample.host_owned {
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
    let native_active = super::super::delivery::native_working(state).unwrap_or(false);
    let host_owned = super::awaiting_host(state)?;
    let mut controller_active = processing_active(&root, &pending.id)? && !host_owned;
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
    let cause = if let Some(error) = host_error
        .as_ref()
        .filter(|e| e.starts_with("REVIEW_EXPANSION_PENDING"))
    {
        super::super::service::cause(error)
    } else if let Some(reason) = rejected {
        reason
    } else if let Some(error) = host_error {
        super::super::service::cause(&error)
    } else if Store::open(&state.worker.session).is_err() {
        "review lock busy".into()
    } else if state.observer.pane.is_none() {
        "observer pane absent".into()
    } else {
        "no current-request native activity or report growth".into()
    };
    controller_active |= cause.starts_with("HOST EXPANSION: ")
        || cause.starts_with("VALIDATION BLOCKED: REVIEW_SOURCE_OUTSIDE_PACKET");
    Ok(Sample {
        request: pending.id.clone(),
        now_ms: kah_observer::session::now_ms(),
        started_ms: pending.created_ms,
        native_active,
        controller_active,
        host_owned,
        transcript_version: state.observer.transcript.as_deref().and_then(version),
        report_version: super::host_progress::combined(&root, &outbox, &pending.id, &report)?,
        cause,
    })
}
pub(super) fn version(path: &Path) -> Option<(u64, u64)> {
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
pub(super) fn report_version(outbox: &Path, request: &str, submitted: &Path) -> Option<(u64, u64)> {
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
    let state = if host_expansion(&p.sample) {
        "host_expansion"
    } else if service_outage(&p.sample) {
        "service_unavailable"
    } else if validation_blocked(&p.sample) {
        "validation_blocked"
    } else if p.stalled && p.sample.host_owned {
        "host_stalled"
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
    if !host_expansion(&p.sample) {
        crate::unattended_report::save(
            root,
            "notice.json",
            &json!({"text":text,"request":p.sample.request,"state":state}),
        )?;
    }
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
    let blocked_state = super::failure::notice_state(state, p)?;
    let prior: Option<Value> = super::super::state::read(&root.join("reviewer.json"))?;
    if super::failure::already_notified(prior.as_ref(), &p.sample.request, blocked_state.as_ref()) {
        return Ok(());
    }
    if prior.as_ref().is_some_and(|v| {
        v["request"] == p.sample.request
            && v["cause"] == p.sample.cause
            && (validation_blocked(&p.sample)
                || service_outage(&p.sample)
                || v["last_progress_ms"] == p.last_progress_ms)
            && v["submitted"] == true
    }) {
        return Ok(());
    }
    let (label, action) = description(&p.sample);
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
        &json!({"request":p.sample.request,"cause":p.sample.cause,"notice_state":blocked_state,"last_progress_ms":p.last_progress_ms,
        "submitted":sent.is_ok(),"error":sent.err().map(|e|e.to_string())}),
    )?;
    Ok(())
}
pub fn notice(worker: &str) -> Result<Option<Value>> {
    super::failure::take_notice(worker)
}

fn validation_notice(state: &State, progress: &Progress) -> Result<()> {
    let root = Store::directory(&state.worker.session)?;
    let checked: Option<Value> = super::super::state::read(&root.join("validation-current.json"))?;
    let current: Option<Value> = super::super::state::read(&root.join("gap.json"))?;
    let Some(mut gap) = current
        .filter(|v| {
            v["request"] == progress.sample.request
                && v["error"].as_str() == progress.sample.cause.strip_prefix("VALIDATION BLOCKED: ")
        })
        .or_else(|| {
            checked.filter(|v| {
                v["request"] == progress.sample.request
                    && v["error"].as_str()
                        == progress.sample.cause.strip_prefix("VALIDATION BLOCKED: ")
            })
        })
    else {
        return Ok(());
    };
    gap["status"] = "validation_blocked".into();
    crate::unattended_report::save(&root, "gap.json", &gap)?;
    notify(state, progress)?;
    if super::super::redelivery::gap_retryable(&gap)
        && !super::super::delivery::retry_held(state, progress.sample.native_active)
        && (!progress.sample.controller_active || host_expansion(&progress.sample))
    {
        super::super::hook::spawn("__review-retry", &state.worker.session)?;
    }

    Ok(())
}

#[cfg(test)]
#[path = "../../../tests/observer_unit/liveness_post_ack_match.rs"]
mod correction_tests;
