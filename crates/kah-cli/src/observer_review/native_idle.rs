//! Herdr can keep the observer pane Working after its native turn has
//! ended, which can prevent the retained request from being delivered.
//! redelivered the retained request. The transcript had turn_duration after a
//! Stop hook and then stayed unchanged. The provider integration
//! reports only SessionStart, so it cannot learn turn end from Claude hooks.
//! A durable ended turn whose transcript has stayed unchanged for QUIET is idle
//! proof. It never interrupts a turn and starts at most one delivery per request.
use super::state::{State, Store};
use crate::Result;
use serde_json::{Value, json};
use std::{path::Path, time::Duration};

const QUIET: Duration = Duration::from_secs(60);

/// What allowed a warm delivery to start.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Ready {
    Herdr,
    Transcript,
}

pub fn ended_quietly(state: &State) -> bool {
    let Some(path) = &state.observer.transcript else {
        return false;
    };
    let quiet = std::fs::metadata(path)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|time| time.elapsed().ok())
        .is_some_and(|age| age >= QUIET);
    quiet
        && std::fs::read_to_string(path).is_ok_and(|text| {
            turn_ended(
                &state.worker.provider,
                &text,
                state.observer.session.as_deref().unwrap_or(""),
            )
        })
}

/// Herdr's screen status, or the transcript proof that started this delivery.
pub fn idle(socket: &Path, pane: &str, ready: Ready) -> Result<bool> {
    if ready == Ready::Transcript {
        return Ok(true);
    }
    let view = crate::relay_transport::herdr(socket, &["pane", "get", pane])?;
    let status = view["result"]["pane"]["agent_status"].as_str();
    Ok(matches!(status, Some("idle" | "done")))
}

/// The last native turn ended and no later user input started another.
pub fn turn_ended(provider: &str, text: &str, session: &str) -> bool {
    if provider == "claude" {
        return kah_claude::lifecycle::completed(text, session).unwrap_or(false);
    }
    let mut ended = false;
    for row in text
        .lines()
        .filter_map(|l| serde_json::from_str::<Value>(l).ok())
    {
        let kind = row["payload"]["type"].as_str();
        if row["type"] == "event_msg" && kind == Some("task_complete") {
            ended = true;
        } else if row["type"] == "event_msg" && kind == Some("task_started")
            || row["type"] == "response_item"
                && kind == Some("message")
                && row["payload"]["role"] == "user"
        {
            ended = false;
        }
    }
    ended
}

/// A repeat would stack the same prompt in a composer that did not accept the
/// first one; after it, Herdr's idle status or the delivery deadline decides.
pub fn available(state: &State) -> Result<bool> {
    let Some(pending) = &state.pending else {
        return Ok(false);
    };
    Ok(ended_quietly(state) && unclaimed(&Store::directory(&state.worker.session)?, &pending.id))
}

/// Spent only when the prompt is actually submitted.
pub fn claim(state: &State) -> Result<()> {
    let pending = state
        .pending
        .as_ref()
        .ok_or("REVIEW_PENDING_MISSING stage=delivery")?;
    record(&Store::directory(&state.worker.session)?, &pending.id)
}

pub fn unclaimed(directory: &Path, request: &str) -> bool {
    !marker(directory, request).exists()
}

pub fn record(directory: &Path, request: &str) -> Result<()> {
    let path = marker(directory, request);
    kah_core::run_store::ensure_private_directory(path.parent().unwrap())?;
    crate::unattended_report::save(
        path.parent().unwrap(),
        &format!("{request}.json"),
        &json!({"request":request,"at_ms":kah_observer::session::now_ms()}),
    )
}

fn marker(directory: &Path, request: &str) -> std::path::PathBuf {
    directory
        .join("transcript-idle")
        .join(format!("{request}.json"))
}

#[cfg(test)]
#[path = "../../tests/observer_unit/native_idle.rs"]
mod tests;
