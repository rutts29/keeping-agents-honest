//! Host-owned stage timestamps, never limits or interruption deadlines.
use super::state::Store;
use crate::Result;
use serde_json::{Value, json};
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
    let name = format!("{request}.json");
    let mut record: Value = super::state::read(&root.join(&name))?
        .unwrap_or_else(|| json!({"worker":worker,"request":request,"at_ms":{}}));
    if record["at_ms"][stage].is_null() {
        record["at_ms"][stage] = at_ms.into();
    }
    super::source_store::save(&root, &name, &record)
}
pub fn now(worker: &str, request: &str, stage: &str) -> Result<()> {
    mark(worker, request, stage, kah_observer::session::now_ms())
}
