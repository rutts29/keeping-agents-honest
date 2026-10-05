//! Lossless recovery of an undelivered review; one fresh snapshot, no lost events.
use super::{
    queue::Wake,
    state::{Purpose, Request, Store},
};
use crate::Result;
use serde_json::{Value, json};
use std::{fs, path::PathBuf};

pub fn context(pending: Option<&Request>, jobs: &[(PathBuf, Wake)], latest: &Value) -> Value {
    let triggers=pending.iter().map(|r|json!({"request":r.id,"purpose":r.purpose,"turn":r.turn,"trigger":r.trigger}))
        .chain(jobs.iter().map(|(path,j)|json!({"record":path,"purpose":j.purpose,"turn":j.turn,"trigger":j.trigger})))
        .collect::<Vec<_>>();
    let actions = triggers
        .iter()
        .flat_map(|t| t["trigger"]["actions"].as_array().into_iter().flatten())
        .cloned()
        .collect::<Vec<_>>();
    json!({"recovery":{"since_review":latest["request"],"since_completed_ms":latest["completed_ms"],
        "pending_request":pending.map(|r|&r.id),"source_records_preserved":true},
        "coalesced_triggers":triggers,"actions":actions})
}

pub fn run(worker: &str) -> Result<Value> {
    super::runtime::require_active(worker)?;
    let store = Store::open_wait(worker)?;
    let mut state = store.load()?.ok_or("REVIEW_STATE_MISSING stage=recovery")?;
    let jobs = super::queue::entries(worker)?;
    let pending = state
        .pending
        .as_ref()
        .map(|p| super::workspace::request(&store, &p.id))
        .transpose()?;
    let mut latest = state.latest.clone().unwrap_or(Value::Null);
    if let Some(id) = latest["request"].as_str() {
        let completed = super::workspace::request(&store, id)?;
        latest["completed_ms"] = json!(
            completed
                .created_ms
                .saturating_add(latest["elapsed_ms"].as_u64().unwrap_or(0))
        );
    }
    let trigger = context(pending.as_ref(), &jobs, &latest);
    let turn = super::control::config(worker)?
        .ok_or("REVIEW_CONFIG_MISSING stage=recovery")?
        .turn;
    let mut request = super::capture::request(&state, Purpose::Drift, &turn, trigger)?;
    request.trigger["last_completed_review"] = latest["request"].clone();
    let archive = store
        .directory
        .join("recovery-records")
        .join(crate::relay_store::nonce());
    kah_core::run_store::ensure_private_directory(&archive)?;
    super::source_store::save(
        &archive,
        "recovery.json",
        &json!({"old_pending":state.pending,
        "new_request":request.id,"records":jobs.iter().map(|(path,_)|path).collect::<Vec<_>>() }),
    )?;
    // Persist new ownership before moving any old record. Later arrivals stay
    // queued separately and get their normal post-snapshot follow-up.
    super::queue::enqueue(
        worker,
        Purpose::Drift,
        &turn,
        json!({"captured_snapshot":request}),
    )?;
    for (path, _) in jobs {
        fs::rename(&path, archive.join(path.file_name().unwrap()))?;
    }
    state.pending = None;
    store.save(&state)?;
    let gap = store.directory.join("gap.json");
    if gap.exists() {
        fs::rename(&gap, archive.join("original-gap.json"))?;
    }
    drop(store);
    super::queue::drain(worker)
}
