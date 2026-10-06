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
    let paths = fs::read_dir(directory)?
        .map(|e| e.map(|e| e.path()))
        .collect::<std::io::Result<Vec<_>>>()?;
    // Both nonce and receipt filenames carry immutable triggers. Filesystem
    // publish timestamps provide their common chronology, independent of names.
    let mut chronological = paths
        .into_iter()
        .map(|path| {
            let time = fs::metadata(&path)?.modified()?;
            Ok((time, path))
        })
        .collect::<std::io::Result<Vec<_>>>()?;
    chronological.sort();
    let mut paths = chronological
        .into_iter()
        .map(|(_, path)| path)
        .collect::<Vec<_>>();
    paths.retain(|p| p.extension().is_some_and(|e| e == "json"));
    let mut jobs = paths
        .into_iter()
        .map(|path| {
            let value: Wake =
                super::state::read(&path)?.ok_or("REVIEW_QUEUE_MISSING stage=queue")?;
            Ok((path, value))
        })
        .collect::<Result<Vec<_>>>()?;
    // Finish the one admitted verification round before newer routine claims
    // can manufacture more rechecks. Every trigger remains durably queued.
    jobs.sort_by_key(|(_, job)| match job.purpose {
        Purpose::Checkpoint
            if job.trigger["verification_followup"]["stage"] == "checks_complete" =>
        {
            0
        }
        Purpose::Checkpoint => 1,
        _ => 2,
    });
    Ok(jobs)
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
    super::events::transfer(worker)?;
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
    super::source_store::save(&directory, &name, &value)?;
    Ok(true)
}
