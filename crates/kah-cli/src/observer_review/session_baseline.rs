//! The session-start HEAD is the aligned baseline: recorded and accepted, never
//! judged as agent work. Only commits made after the session start are reviewed.
use crate::Result;
use serde_json::{Value, json};
use std::path::Path;

/// The HEAD captured synchronously by SessionStart is the baseline. Seed that
/// exact HEAD even when the first asynchronous sample sees a later commit.
/// Without that capture (a resumed session, a failed capture, or a capture
/// whose HEAD read failed and so has no `head` field), `start`
/// gives the HEAD's commit time and the session start, both in ms; a HEAD
/// committed no later than the start is accepted. Existing review baselines
/// are never replaced.
pub fn accept(
    root: &Path,
    head: &str,
    start: impl FnOnce(&str) -> Result<(Option<u64>, Option<u64>)>,
) -> Result<bool> {
    let captured: Option<Value> =
        super::state::read(&root.join("artifact-baseline/baseline.json"))?;
    let exact = |c: &Value| c["initial"] == true && c.get("head").is_some();
    if let Some(captured) = captured.filter(exact) {
        let Some(initial) = captured["head"].as_str() else {
            return Ok(false);
        };
        seed_both(root, initial, &captured["captured_ms"])?;
        return Ok(head == initial);
    }
    let (Some(committed), Some(started)) = start(head)? else {
        return Ok(false);
    };
    if committed > started {
        return Ok(false);
    }
    seed_both(root, head, &json!(started))?;
    Ok(true)
}
fn seed_both(root: &Path, head: &str, started_ms: &Value) -> Result<()> {
    seed(
        root,
        "option1-baseline.json",
        &json!({"head":head,"reviewed":"session_start","started_ms":started_ms}),
    )?;
    seed(
        root,
        "option1-checkpoint-baseline.json",
        &json!({"head":head,"covered_commits":[],"reviewed":"session_start"}),
    )
}
fn seed(root: &Path, name: &str, value: &Value) -> Result<()> {
    if super::state::read::<Value>(&root.join(name))?.is_none() {
        crate::unattended_report::save(root, name, value)?;
    }
    Ok(())
}
