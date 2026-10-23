//! Host-owned stage timestamps, never limits or interruption deadlines.
use super::state::Store;
use crate::Result;
use serde_json::{Value, json};
use std::{fs::OpenOptions, io::Write, os::unix::fs::OpenOptionsExt};
pub fn mark(worker: &str, request: &str, stage: &str, at_ms: u64) -> Result<()> {
    if [request, stage].iter().any(|s| {
        s.is_empty()
            || !s
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_'))
    }) {
        return Err("REVIEW_TIMING_ID stage=telemetry".into());
    }
    let root = Store::directory(worker)?.join("stage-timings");
    kah_core::run_store::ensure_private_directory(&root)?;
    let _lock = Store::delivery_at(&root, true)?;
    let mut history = OpenOptions::new()
        .create(true)
        .append(true)
        .mode(0o600)
        .open(root.join(format!("{request}.jsonl")))?;
    writeln!(
        history,
        "{}",
        json!({"worker":worker,"request":request,"stage":stage,"at_ms":at_ms,"writer_pid":std::process::id()})
    )?;
    history.sync_all()?;
    let name = format!("{request}.json");
    let mut record: Value = super::state::read(&root.join(&name))?
        .unwrap_or_else(|| json!({"worker":worker,"request":request,"at_ms":{}}));
    if record["at_ms"][stage].is_null() {
        record["at_ms"][stage] = at_ms.into();
    }
    super::source_store::save(&root, &name, &record)
}
/// Stop means the native main agent finished its response (Claude Code hooks
/// docs; the Codex adapter validates Stop).
/// This is the HOST RECEIPT time, not an invented provider timestamp.
pub fn native_end(worker: &str, event: &super::hook_queue::Event) -> Result<()> {
    if event.input["hook_event_name"] != "Stop" {
        return Ok(());
    }
    let (Some(context), Some(at)) = (&event.context, event.received_ms) else {
        return Ok(());
    };
    let request = super::preparation::batch::load(worker, &context.request)?
        .map(|batch| batch.review)
        .unwrap_or_else(|| context.request.clone());
    mark(worker, &request, "native_done", at)
}
pub fn now(worker: &str, request: &str, stage: &str) -> Result<()> {
    mark(worker, request, stage, kah_observer::session::now_ms())
}
