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
/// Returns whether a deferral was recorded.
pub fn record(directory: &Path, input: &kah_observer::commit_check::Input) -> Result<bool> {
    let Some(deferred) = input.carried.get("deferred_obligations") else {
        return Ok(false);
    };
    crate::unattended_report::save(
        directory,
        MARKER,
        &json!({"head":input.revision,"rules":deferred["rules"]}),
    )?;
    Ok(true)
}

/// Commit admission: record the deferral. If the commit's own turn already
/// ended, that turn's end check ran before the deferral existed (admission
/// waited behind the cold current-rules review), so run the check here.
/// Only the worker's native transcript tells, never kah's Stop
/// path; the review is still queued once per HEAD.
pub fn admitted(
    home: &Path,
    state: &super::state::State,
    root: &Path,
    input: &kah_observer::commit_check::Input,
) -> Result<()> {
    if record(root, input)? {
        // Best effort: any failure keeps today's behaviour, with no new error,
        // gap or stage failure on the admission path.
        let _ = recheck(home, state, &input.revision);
    }
    Ok(())
}
fn recheck(home: &Path, state: &super::state::State, head: &str) -> Result<()> {
    if !catch_up_applies(&state.worker.project) {
        return Ok(());
    }
    let worker = &state.worker.session;
    if turn_ended_since(
        &super::native_turn_end::worker(home, state),
        super::tags::committed_ms(home, state, head),
    ) && queue_due(worker, home)?
    {
        queue::schedule(worker)?;
    }
    Ok(())
}

/// Only interactive sessions need the catch-up. An unattended `kah run` worker
/// has a run-completion checkpoint ahead that reviews every rule on the
/// delivered head (due() is false once a checkpoint ran there), so the late
/// order is harmless there and it keeps the existing path.
pub fn catch_up_applies(project: &Path) -> bool {
    super::execution_fact::run_domain(project).is_none()
}

/// Whether a failed checkpoint's notice is pushed into the worker's pane. A
/// delivered-change review runs after an interactive worker's turn ended, so
/// no hook would take a queued reminder. Other checkpoints and
/// kah run workers keep the queued reminder.
pub fn push_notice(trigger: &Value, project: &Path) -> bool {
    std::iter::once(trigger)
        .chain(trigger["events"].as_array().into_iter().flatten())
        .any(|event| event["kind"] == "delivered_change")
        && catch_up_applies(project)
}

/// The text pushed into the pane. Typed into the worker's prompt, it must be
/// a kah delegate message (like advance::correction's '[kah-observer]'), or kah
/// captures its own notice as user intent.
pub fn pushed_text(notice: &str) -> String {
    format!("[kah-observer] {notice}")
}

/// The pushed text for this worker; it is what is typed, so it is what must
/// fit the native size limit.
pub fn typed_notice(provider: &str, notice: &str) -> Result<String> {
    let typed = pushed_text(notice);
    super::reminder::validate_native(provider, &typed)?;
    Ok(typed)
}

/// Whether a native turn ended at or after the commit. Git's commit time has
/// one-second precision, so a previous turn's end earlier in the commit's own
/// second also counts. That needs a new prompt plus a commit within one
/// second; the cost is an early delivered-change review, never a missed one.
pub fn turn_ended_since(turn_ends_ms: &[u64], committed_ms: Option<u64>) -> bool {
    committed_ms.is_some_and(|commit| turn_ends_ms.iter().any(|&end| end >= commit))
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
