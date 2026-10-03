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
    crate::unattended_report::save(
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
    let Some((path, job)) = entries(worker)?.into_iter().next() else {
        return Ok(json!({"status":"idle"}));
    };
    let receipt = super::runtime::submit(worker, job.purpose, &job.turn, job.trigger)?;
    if matches!(
        receipt["status"].as_str(),
        Some("submitted" | "preparation_submitted")
    ) {
        fs::remove_file(path)?;
    }
    Ok(receipt)
}
pub fn kick(worker: &str) -> Result<()> {
    if Store::read(worker)?.is_some_and(|s| s.pending.is_some()) {
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
