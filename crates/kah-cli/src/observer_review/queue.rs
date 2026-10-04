//! Durable triggers outlive the current review and its lock; no turn ceiling.
use super::state::{Purpose, Store};
use crate::Result;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{fs, path::PathBuf};

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Wake {
    pub purpose: Purpose,
    pub turn: String,
    pub trigger: Value,
}
pub fn enqueue(worker: &str, purpose: Purpose, turn: &str, trigger: Value) -> Result<()> {
    let directory = Store::directory(worker)?.join("queued");
    kah_core::run_store::ensure_private_directory(&directory)?;
    kah_core::run_store::ensure_private_directory(&Store::directory(worker)?.join("sources"))?;
    super::source_store::save(
        &directory,
        &format!("{}.json", crate::relay_store::nonce()),
        &serde_json::to_value(Wake {
            purpose,
            turn: turn.into(),
            trigger,
        })?,
    )
}
pub fn entries(worker: &str) -> Result<Vec<(PathBuf, Wake)>> {
    let directory = Store::directory(worker)?.join("queued");
    if !directory.exists() {
        return Ok(vec![]);
    }
    let mut paths = fs::read_dir(directory)?
        .map(|e| e.map(|e| e.path()))
        .collect::<std::io::Result<Vec<_>>>()?;
    paths.sort();
    paths.retain(|p| p.extension().is_some_and(|e| e == "json"));
    paths
        .into_iter()
        .map(|path| {
            let value = super::state::read(&path)?.ok_or("REVIEW_QUEUE_MISSING stage=queue")?;
            Ok((path, value))
        })
        .collect()
}
pub fn drain(worker: &str) -> Result<Value> {
    if let Some(state) = Store::read(worker)?
        && state.pending.is_some()
    {
        if state.stopped {
            super::runtime::resume(worker)?;
        }
        return Ok(json!({"status":"queued","pending":state.pending}));
    }
    let mut jobs = entries(worker)?;
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
    if let Some((job, paths)) = batch(&jobs) {
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
            for path in paths {
                fs::rename(&path, handled.join(path.file_name().unwrap()))?;
            }
        }
        return Ok(receipt);
    }
    Ok(json!({"status":"idle"}))
}
/// Snapshot only existing queued events. Later events retain their own files
/// and cause a follow-up; the in-flight review is never mutated.
pub fn batch(jobs: &[(PathBuf, Wake)]) -> Option<(Wake, Vec<PathBuf>)> {
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
    if super::questions::pending(&Store::directory(worker)?)?
        .iter()
        .any(|q| q["delivery"] != "submitted")
    {
        super::hook::spawn("__review-questions", worker)?;
    }
    if Store::read(worker)?.is_some_and(|s| s.pending.is_some()) {
        super::hook::spawn("__review-retry", worker)?;
        return Ok(());
    }
    if !entries(worker)?.is_empty() {
        super::hook::spawn("__review-wake", worker)?;
    }
    if super::events::pending(worker)? {
        super::hook::spawn("__checkpoint-worker", worker)?;
    }
    Ok(())
}
