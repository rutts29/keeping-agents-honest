//! New worker events wake pending interpretation; completion never spins a retry loop.
use crate::{
    Result,
    observer_review::{
        queue::Wake,
        state::{Request, Store},
    },
};
use serde_json::{Value, json};
use std::{
    fs,
    path::{Path, PathBuf},
};

pub fn retain_rethink(root: &Path, request: &Request, assessment: &Value) -> Result<()> {
    if !assessment["needs_thinker"]
        .as_array()
        .is_some_and(|a| !a.is_empty())
    {
        return Ok(());
    }
    // Apply the one-round verification condition:
    // a rethought checkpoint proceeds through verification, not another ask.
    if request.purpose == super::super::state::Purpose::Checkpoint
        && request.trigger["reconsideration"].is_object()
    {
        return Ok(());
    }
    let directory = root.join("rethink");
    kah_core::run_store::ensure_private_directory(&directory)?;
    kah_core::run_store::ensure_private_directory(&root.join("sources"))?;
    let mut trigger = request.trigger.clone();
    trigger["reconsideration"] = json!({"request":request.id,"needs_thinker":assessment["needs_thinker"],"result":root.join("results").join(format!("{}.json",request.id))});
    super::super::source_store::save(
        &directory,
        &format!("{}.json", request.id),
        &serde_json::to_value(Wake {
            purpose: request.purpose.clone(),
            turn: request.turn.clone(),
            trigger,
        })?,
    )
}
fn entries(root: &Path) -> Result<Vec<PathBuf>> {
    let directory = root.join("rethink");
    if !directory.exists() {
        return Ok(vec![]);
    }
    let mut paths = fs::read_dir(directory)?
        .map(|e| e.map(|e| e.path()))
        .collect::<std::io::Result<Vec<_>>>()?;
    paths.retain(|p| p.extension().is_some_and(|e| e == "json"));
    paths.sort();
    Ok(paths)
}
pub fn rethink(root: &Path) -> Result<Option<Wake>> {
    entries(root)?
        .first()
        .map(|p| super::super::state::read(p))
        .transpose()
        .map(Option::flatten)
}
pub fn schedule(worker: &str) -> Result<()> {
    let root = Store::directory(worker)?;
    if !entries(&root)?.is_empty() || !super::super::questions::pending(&root)?.is_empty() {
        super::super::hook::spawn("__review-rethink", worker)?;
    }
    Ok(())
}
pub fn resume(worker: &str) -> Result<()> {
    super::super::runtime::require_active(worker)?;
    let store = Store::open_wait(worker)?;
    super::super::questions::queue_answers(&store, worker)?;
    let handled = store.directory.join("covered-rethinks");
    kah_core::run_store::ensure_private_directory(&handled)?;
    for path in entries(&store.directory)? {
        let wake: Wake =
            super::super::state::read(&path)?.ok_or("REVIEW_RETHINK_MISSING stage=rethink")?;
        super::super::queue::enqueue(worker, wake.purpose, &wake.turn, wake.trigger)?;
        fs::rename(&path, handled.join(path.file_name().unwrap()))?;
    }
    drop(store);
    super::super::queue::kick(worker)
}
