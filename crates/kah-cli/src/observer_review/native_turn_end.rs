//! When the worker's native turns ended, read from its own transcript. Claude
//! writes turn_duration right after its Stop hooks, and Codex writes
//! task_complete after the Stop hooks of the turn (0.162.1 session/turn.rs),
//! independent of kah's delayed Stop handling, so a later turn cannot hide an
//! earlier turn's end.
use super::state::State;
use serde_json::Value;
use std::path::Path;

/// Turn-end times of the worker, in ms. Empty whenever the transcript is
/// unreadable.
pub fn worker(home: &Path, state: &State) -> Vec<u64> {
    let session = &state.worker.session;
    match state.worker.provider.as_str() {
        "claude" => crate::claude_transcript::resolve(home, &state.worker.project, session, None)
            .ok()
            .and_then(|path| std::fs::read_to_string(path).ok())
            .map(|text| claude(&text, session))
            .unwrap_or_default(),
        "codex" => kah_core::run_store::RunStore::default_state_root()
            .ok()
            .and_then(|root| {
                kah_observer::session::Store::read(&root, session)
                    .ok()
                    .flatten()
            })
            .map(|state| codex_rollout(&state.binding.codex_home, session))
            .unwrap_or_default(),
        _ => vec![],
    }
}

/// turn_duration rows of `session`'s main thread, as Unix ms.
pub fn claude(text: &str, session: &str) -> Vec<u64> {
    rows(text)
        .filter(|row| {
            row["type"] == "system"
                && row["subtype"] == "turn_duration"
                && row["isSidechain"] != true
                && row["sessionId"] == session
        })
        .filter_map(|row| unix_ms(&row))
        .collect()
}

/// task_complete rows of `session`'s own rollout under `codex_home`, as Unix ms.
pub fn codex_rollout(codex_home: &Path, session: &str) -> Vec<u64> {
    super::native_receipt::locate(&codex_home.join("sessions"), session)
        .ok()
        .flatten()
        .and_then(|path| std::fs::read_to_string(path).ok())
        .map(|text| codex(&text))
        .unwrap_or_default()
}

/// task_complete rows of one rollout, as Unix ms. An aborted turn is not a
/// completed turn.
pub fn codex(text: &str) -> Vec<u64> {
    rows(text)
        .filter(|row| row["type"] == "event_msg" && row["payload"]["type"] == "task_complete")
        .filter_map(|row| unix_ms(&row))
        .collect()
}

fn rows(text: &str) -> impl Iterator<Item = Value> + '_ {
    text.lines()
        .filter_map(|line| serde_json::from_str::<Value>(line).ok())
}

fn unix_ms(row: &Value) -> Option<u64> {
    use time::{OffsetDateTime, format_description::well_known::Rfc3339};
    let at = OffsetDateTime::parse(row["timestamp"].as_str()?, &Rfc3339).ok()?;
    u64::try_from(at.unix_timestamp_nanos() / 1_000_000).ok()
}
