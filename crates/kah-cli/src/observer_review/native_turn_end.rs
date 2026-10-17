//! When the worker's native turns ended, read from its own transcript. Claude
//! writes turn_duration right after its Stop hooks, independent of kah's
//! delayed Stop handling, so a later turn cannot hide an earlier turn's end.
use super::state::State;
use std::path::Path;

/// Turn-end times of the worker, in ms. Empty for Codex workers (their turn
/// end needs the app-server) and whenever the transcript is unreadable.
pub fn worker(home: &Path, state: &State) -> Vec<u64> {
    if state.worker.provider != "claude" {
        return vec![];
    }
    crate::claude_transcript::resolve(home, &state.worker.project, &state.worker.session, None)
        .ok()
        .and_then(|path| std::fs::read_to_string(path).ok())
        .map(|text| claude(&text, &state.worker.session))
        .unwrap_or_default()
}

/// turn_duration rows of `session`'s main thread, as Unix ms.
pub fn claude(text: &str, session: &str) -> Vec<u64> {
    use time::{OffsetDateTime, format_description::well_known::Rfc3339};
    text.lines()
        .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
        .filter(|row| {
            row["type"] == "system"
                && row["subtype"] == "turn_duration"
                && row["isSidechain"] != true
                && row["sessionId"] == session
        })
        .filter_map(|row| OffsetDateTime::parse(row["timestamp"].as_str()?, &Rfc3339).ok())
        .filter_map(|at| u64::try_from(at.unix_timestamp_nanos() / 1_000_000).ok())
        .collect()
}
