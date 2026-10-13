//! Obligations deferred from commit reviews are judged on the delivered change.
//! When the worker's turn ends after such a commit review, queue exactly one
//! checkpoint over the cumulative diff from the session baseline per HEAD.
use super::{
    queue::{self, Wake},
    state::{Purpose, Store},
};
use crate::Result;
use serde_json::{Value, json};
use std::path::Path;

/// Durable fact: a reviewed commit deferred these obligations.
pub const MARKER: &str = "deferred-obligations.json";
const BASELINE: &str = "option1-checkpoint-baseline.json";

/// Record the deferral of a commit review. Nothing is written without one.
pub fn record(directory: &Path, input: &kah_observer::commit_check::Input) -> Result<()> {
    let Some(deferred) = input.carried.get("deferred_obligations") else {
        return Ok(());
    };
    crate::unattended_report::save(
        directory,
        MARKER,
        &json!({"head":input.revision,"rules":deferred["rules"]}),
    )
}

/// Pure decision. `marker` is the deferral record, `baseline` the checkpoint
/// baseline and `checkpoint` the last checkpoint state.
pub fn due(
    marker: Option<&Value>,
    head: &str,
    baseline: Option<&Value>,
    checkpoint: Option<&Value>,
) -> bool {
    let Some(marker) = marker else {
        return false;
    };
    if marker["rules"].as_array().is_none_or(|r| r.is_empty()) {
        return false;
    }
    let covered = |h: &Value| {
        baseline.is_some_and(|b| {
            b["covered_commits"]
                .as_array()
                .is_some_and(|c| c.contains(h))
        })
    };
    !covered(&marker["head"])
        && baseline.is_none_or(|b| b["head"] != head)
        && checkpoint.is_none_or(|c| c["head"] != head)
}

/// The checkpoint wake: a non-commit class, so admission reads the checkpoint
/// baseline and judges the cumulative change with checkpoint=true.
pub fn wake(turn: &str, head: &str, marker: &Value) -> Wake {
    Wake {
        purpose: Purpose::Checkpoint,
        turn: turn.into(),
        trigger: json!({"review_class":"gate","head":head,"events":[{"kind":"delivered_change",
            "head":head,"deferred_obligations":marker["rules"],"source":"controller turn end"}]}),
    }
}

/// Queue the wake once per HEAD. Returns true when a new wake was queued.
pub fn queue_once(root: &Path, turn: &str, head: &str, marker: &Value) -> Result<bool> {
    queue::enqueue_once_at(
        root,
        &format!("delivered-change:{head}"),
        &wake(turn, head, marker),
    )
}

/// Turn end: queue the delivered-change review when it is due, then kick the
/// queue off the native hook.
pub fn schedule(worker: &str, home: &Path) -> Result<()> {
    if queue_due(worker, home)? {
        queue::schedule(worker)?;
    }
    Ok(())
}
/// Queue the delivered-change review when it is due. Returns true when queued.
pub fn queue_due(worker: &str, home: &Path) -> Result<bool> {
    let root = Store::directory(worker)?;
    let Some(marker) = super::state::read::<Value>(&root.join(MARKER))? else {
        return Ok(false);
    };
    let Some(state) = Store::read(worker)? else {
        return Ok(false);
    };
    // As tags::run: a pending worktree is sampled again at the next turn end.
    let head = match super::tags::head(home, &state) {
        Ok(Some(head)) => head,
        Ok(None) => return Ok(false),
        Err(e) if e.to_string().starts_with("REVIEW_WORKTREE_PENDING") => return Ok(false),
        Err(e) => return Err(e),
    };
    let baseline: Option<Value> = super::state::read(&root.join(BASELINE))?;
    if !due(
        Some(&marker),
        &head,
        baseline.as_ref(),
        state.checkpoint.as_ref(),
    ) {
        return Ok(false);
    }
    let turn = super::control::config(worker)?
        .map(|c| c.turn)
        .unwrap_or_else(|| "checkpoint".into());
    queue_once(&root, &turn, &head, &marker)
}
