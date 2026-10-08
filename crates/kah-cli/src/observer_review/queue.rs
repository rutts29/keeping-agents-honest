//! Durable triggers outlive the current review and its lock; no turn ceiling.
use super::state::{Purpose, Store};
use crate::Result;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{fs, path::PathBuf};
mod drift;
mod identity;
pub mod lanes;
pub mod metadata;
pub mod migration;
mod ordering;
pub mod preparation_coverage;
pub mod repair;
pub mod supersede;

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Wake {
    pub purpose: Purpose,
    pub turn: String,
    pub trigger: Value,
}
pub fn enqueue(worker: &str, purpose: Purpose, turn: &str, trigger: Value) -> Result<()> {
    let root = Store::directory(worker)?;
    let _lock = Store::delivery_at(&root.join("queue-receipts"), true)?;
    let directory = root.join("queued");
    kah_core::run_store::ensure_private_directory(&directory)?;
    kah_core::run_store::ensure_private_directory(&Store::directory(worker)?.join("sources"))?;
    if purpose == Purpose::Drift && trigger.get("captured_snapshot").is_none() {
        return drift::append(&root, turn, trigger);
    }
    let name = if purpose == Purpose::Steering {
        identity::steering(&trigger)?
            .unwrap_or_else(|| format!("{}.json", crate::relay_store::nonce()))
    } else {
        format!("{}.json", crate::relay_store::nonce())
    };
    if purpose == Purpose::Steering && identity::retained(&root, &name, &trigger)? {
        return Ok(());
    }
    if purpose == Purpose::Checkpoint
        && trigger["review_class"] == "commit"
        && trigger["head"].as_str().is_some()
    {
        return supersede::publish(
            worker,
            &root,
            &name,
            Wake {
                purpose,
                turn: turn.into(),
                trigger,
            },
        );
    }
    metadata::queued(
        &root,
        &name,
        &Wake {
            purpose: purpose.clone(),
            turn: turn.into(),
            trigger: trigger.clone(),
        },
    )?;
    ordering::reserve(&root, &name)?;
    super::source_store::save(
        &directory,
        &name,
        &serde_json::to_value(Wake {
            purpose,
            turn: turn.into(),
            trigger,
        })?,
    )
}
pub fn entries(worker: &str) -> Result<Vec<(PathBuf, Wake)>> {
    let root = Store::directory(worker)?;
    let _lock = Store::delivery_at(&root.join("queue-receipts"), true)?;
    entries_at(&root)
}
pub fn status(worker: &str) -> Result<Value> {
    let root = Store::directory(worker)?;
    kah_core::run_store::ensure_private_directory(&root.join("sources"))?;
    let _lock = Store::delivery_at(&root.join("queue-receipts"), true)?;
    let jobs = entries_at(&root)?;
    let stamps = metadata::stamps(&root, &jobs)?;
    let now = kah_observer::session::now_ms();
    let mut lanes = serde_json::Map::new();
    for lane in [lanes::Lane::Intent, lanes::Lane::Commit, lanes::Lane::Gate] {
        let waiting = jobs
            .iter()
            .filter(|(_, w)| lanes::Lane::of(w) == lane)
            .collect::<Vec<_>>();
        let oldest = waiting
            .first()
            .and_then(|(p, _)| stamps.get(p.file_name()?.to_str()?))
            .map(|s| s.at_ms);
        let key = serde_json::to_value(lane)?.as_str().unwrap().to_owned();
        lanes.insert(key,json!({"depth":waiting.len(),"oldest_at_ms":oldest,"oldest_age_ms":oldest.map(|at|now.saturating_sub(at))}));
    }
    Ok(json!({"at_ms":now,"last_admitted_lane":metadata::last(&root)?,"lanes":lanes}))
}
fn entries_at(root: &std::path::Path) -> Result<Vec<(PathBuf, Wake)>> {
    supersede::recover(root)?;
    let directory = root.join("queued");
    if !directory.exists() {
        return Ok(vec![]);
    }
    let mut paths = fs::read_dir(directory)?
        .map(|e| e.map(|e| e.path()))
        .collect::<std::io::Result<Vec<_>>>()?;
    paths.retain(|p| p.extension().is_some_and(|e| e == "json"));
    let order = ordering::record(root, &paths)?;
    paths.sort_by_key(|p| {
        order
            .get(p.file_name().unwrap().to_str().unwrap())
            .copied()
            .unwrap_or(u64::MAX)
    });
    let mut jobs = vec![];
    for path in paths {
        if let Some(value) = super::state::read::<Wake>(&path)? {
            jobs.push((path, value));
        }
    }
    let stamps = metadata::stamps(root, &jobs)?;
    lanes::sort(
        &mut jobs,
        &stamps,
        metadata::last(root)?,
        kah_observer::session::now_ms(),
    )?;
    Ok(jobs)
}
pub fn drain(worker: &str) -> Result<Value> {
    if super::service::blocked(worker)? {
        return Ok(
            json!({"status":"service_unavailable","error":"Jev unavailable: payment required","queued_retained":true}),
        );
    }
    if let Some(state) = Store::read(worker)?
        && state.pending.is_some()
    {
        if state.stopped {
            super::runtime::resume(worker)?;
        }
        return Ok(json!({"status":"queued","pending":state.pending}));
    }
    preparation_coverage::run(worker)?;
    super::events::transfer(worker)?;
    let claimed = claim_batch(worker)?;
    if let Some((job, paths)) = claimed {
        return submit_batch(worker, job, paths);
    }
    Ok(json!({"status":"idle"}))
}
pub fn claim_batch(worker: &str) -> Result<Option<(Wake, Vec<PathBuf>)>> {
    let root = Store::directory(worker)?;
    let _lock = Store::delivery_at(&root.join("queue-receipts"), true)?;
    if Store::read(worker)?.is_some_and(|s| s.pending.is_some()) {
        return Ok(None);
    }
    let mut jobs = entries_at(&root)?;
    while let Some((path, job)) = jobs.first() {
        if job.purpose == Purpose::Steering && super::steering::covered(worker, &job.trigger)? {
            let handled = Store::directory(worker)?.join("covered-triggers");
            kah_core::run_store::ensure_private_directory(&handled)?;
            fs::rename(path, handled.join(path.file_name().unwrap()))?;
            jobs.remove(0);
            continue;
        }
        break;
    }
    let Some((mut job, mut paths)) = batch(&jobs) else {
        return Ok(None);
    };
    let stamps = metadata::stamps(&root, &jobs)?;
    job.trigger["queued_at_ms"] = json!(
        paths
            .iter()
            .filter_map(|p| stamps.get(p.file_name()?.to_str()?))
            .map(|s| s.at_ms)
            .min()
    );
    drift::detach(&root, &mut paths)?;
    Ok(Some((job, paths)))
}
fn submit_batch(worker: &str, job: Wake, paths: Vec<PathBuf>) -> Result<Value> {
    let submitted = if let Some(snapshot) = job.trigger.get("captured_snapshot") {
        super::snapshot::submit(worker, snapshot)
    } else {
        super::runtime::submit(worker, job.purpose, &job.turn, job.trigger)
    };
    let receipt = match submitted {
        Ok(receipt) => receipt,
        Err(error) if error.to_string().starts_with("REVIEW_BUSY") => {
            return Ok(json!({"status":"queued","reason":"review_store_busy"}));
        }
        Err(error) => return Err(error),
    };
    if matches!(
        receipt["status"].as_str(),
        Some("submitted" | "preparation_submitted")
    ) {
        let handled = Store::directory(worker)?.join("covered-triggers");
        kah_core::run_store::ensure_private_directory(&handled)?;
        let _lock = Store::delivery_at(&Store::directory(worker)?.join("queue-receipts"), true)?;
        for path in paths {
            fs::rename(&path, handled.join(path.file_name().unwrap()))?;
        }
    }
    Ok(receipt)
}
/// Snapshot only existing queued events. Later events retain their own files
/// and cause a follow-up; the in-flight review is never mutated.
pub fn batch(jobs: &[(PathBuf, Wake)]) -> Option<(Wake, Vec<PathBuf>)> {
    if let Some(batch) = super::preparation::batch::packets::group(jobs) {
        return Some(batch);
    }
    let first = jobs.first()?;
    if first.1.purpose != Purpose::Drift || first.1.trigger.get("captured_snapshot").is_some() {
        return Some((first.1.clone(), vec![first.0.clone()]));
    }
    let group = jobs
        .iter()
        .filter(|(_, job)| {
            job.purpose == Purpose::Drift && job.trigger.get("captured_snapshot").is_none()
        })
        .collect::<Vec<_>>();
    let mut job = group.last()?.1.clone();
    job.trigger["coalesced_triggers"] = json!(
        group
            .iter()
            .map(|(_, job)| &job.trigger)
            .collect::<Vec<_>>()
    );
    job.trigger["actions"] = json!(
        group
            .iter()
            .flat_map(|(_, job)| job.trigger["actions"].as_array().into_iter().flatten())
            .collect::<Vec<_>>()
    );
    Some((job, group.iter().map(|(path, _)| path.clone()).collect()))
}
pub fn kick(worker: &str) -> Result<()> {
    if super::gate_approval::pending_followups(worker)? {
        super::hook::spawn("__gate-followup", worker)?;
    }
    if super::gate_approval::pending_answers(worker)? {
        super::hook::spawn("__gate-answers", worker)?;
    }
    if super::questions::pending(&Store::directory(worker)?)?
        .iter()
        .any(|q| q["delivery"] != "submitted")
    {
        super::hook::spawn("__review-questions", worker)?;
    }
    if Store::read(worker)?.is_some_and(|s| s.pending.is_some()) {
        super::liveness::start(worker)?;
        super::hook::spawn("__review-retry", worker)?;
        return Ok(());
    }
    if super::events::pending(worker)? {
        super::hook::spawn("__checkpoint-worker", worker)?;
    } else if !entries(worker)?.is_empty() {
        super::hook::spawn("__review-wake", worker)?;
    }
    Ok(())
}

/// External operations already ran; keep one immutable review hand-off across
/// a crash before or after transfer to covered-triggers.
pub fn enqueue_once_at(root: &std::path::Path, receipt: &str, wake: &Wake) -> Result<bool> {
    use sha2::{Digest, Sha256};
    let _lock = Store::delivery_at(&root.join("queue-receipts"), true)?;
    let name = format!("receipt-{:x}.json", Sha256::digest(receipt.as_bytes()));
    let value = serde_json::to_value(wake)?;
    for directory in ["queued", "covered-triggers"] {
        let path = root.join(directory).join(&name);
        if let Some(saved) = super::state::read::<Value>(&path)? {
            if saved != value {
                return Err("REVIEW_QUEUE_RECEIPT_CHANGED stage=queue".into());
            }
            return Ok(false);
        }
    }
    let directory = root.join("queued");
    kah_core::run_store::ensure_private_directory(&directory)?;
    ordering::reserve(root, &name)?;
    metadata::queued(root, &name, wake)?;
    super::source_store::save(&directory, &name, &value)?;
    Ok(true)
}

/// Native hooks schedule host work without inspecting or waiting on admission.
pub fn schedule(worker: &str) -> Result<()> {
    super::hook::spawn("__review-kick", worker)
}
