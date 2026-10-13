//! The session-start HEAD is the aligned baseline: recorded and accepted, never
//! judged as agent work. Only commits made after the session start are reviewed.
use crate::Result;
use serde_json::{Value, json};
use std::path::Path;

/// Accept `head` as the session-start baseline when it was committed no later
/// than the session's first sample request. A HEAD committed after that is
/// agent work and stays reviewed. Existing review baselines are never replaced.
pub fn accept(
    root: &Path,
    head: &str,
    committed_s: Option<u64>,
    started_ms: Option<u64>,
) -> Result<bool> {
    let (Some(committed), Some(started)) = (committed_s, started_ms) else {
        return Ok(false);
    };
    if committed.saturating_mul(1000) > started {
        return Ok(false);
    }
    seed(
        root,
        "option1-baseline.json",
        &json!({"head":head,"reviewed":"session_start","started_ms":started}),
    )?;
    seed(
        root,
        "option1-checkpoint-baseline.json",
        &json!({"head":head,"covered_commits":[],"reviewed":"session_start"}),
    )?;
    Ok(true)
}
fn seed(root: &Path, name: &str, value: &Value) -> Result<()> {
    if super::state::read::<Value>(&root.join(name))?.is_none() {
        crate::unattended_report::save(root, name, value)?;
    }
    Ok(())
}
