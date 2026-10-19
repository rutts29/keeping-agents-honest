//! Bind missing native callbacks only to an owned pane's verified native receipt.
use crate::Result;
mod recovery;
pub use recovery::ensure;
pub(crate) use recovery::locate;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{io::BufRead, path::Path};

/// An unbound observer that kah launched is still starting; nothing to bind yet.
pub const STARTING: &str = "REVIEW_NATIVE_STARTING";
pub fn starting(error: &dyn std::fmt::Display) -> bool {
    error.to_string().starts_with(STARTING)
}
/// Watcher and retry jobs neither report nor retry while the observer starts;
/// cold delivery owns startup failure reporting.
pub fn ensure_started(worker: &str) -> Result<bool> {
    match ensure(worker) {
        Err(error) if starting(&error) => Ok(false),
        other => other.map(|()| true),
    }
}
/// Cold delivery waits through startup and unbound phases. A first-seen phase
/// is progress, so the window measures a stalled launch, not total launch time.
pub fn startup_progress(seen: &mut Vec<String>, error: &dyn std::fmt::Display) -> Option<bool> {
    let text = error.to_string();
    if !starting(&text) && !text.starts_with("REVIEW_NATIVE_UNBOUND") {
        return None;
    }
    let new = !seen.contains(&text);
    if new {
        seen.push(text);
    }
    Some(new)
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Proof {
    pub session: String,
    pub marker: String,
    pub metadata_digest: String,
    pub receipt_digest: String,
    pub receipt_offset: u64,
    pub receipt_end: u64,
}
fn verify(
    mut reader: impl BufRead,
    session: &str,
    cwd: &Path,
    marker: &str,
) -> Result<Option<Proof>> {
    let mut line = String::new();
    if reader.read_line(&mut line)? == 0 {
        return Ok(None);
    }
    let metadata: Value = serde_json::from_str(&line).map_err(|e| {
        format!("REVIEW_NATIVE_UNBOUND stage=observer_binding: native metadata is not complete JSON: {e}")
    })?;
    if metadata["type"] != "session_meta" || !matches_meta(&metadata["payload"], session, cwd) {
        return Err("REVIEW_NATIVE_UNBOUND stage=observer_binding: session metadata differs from owned pane".into());
    }
    let metadata_digest = super::report_revision::digest(line.as_bytes());
    let mut offset = line.len() as u64;
    loop {
        line.clear();
        let bytes = reader.read_line(&mut line)?;
        if bytes == 0 {
            return Ok(None);
        }
        if super::delivery::receipt(std::io::Cursor::new(&line), "codex", session, marker)? {
            return Ok(Some(Proof {
                session: session.into(),
                marker: marker.into(),
                metadata_digest,
                receipt_digest: super::report_revision::digest(line.as_bytes()),
                receipt_offset: offset,
                receipt_end: offset + bytes as u64,
            }));
        }
        offset += bytes as u64;
    }
}
fn matches_meta(meta: &Value, session: &str, cwd: &Path) -> bool {
    meta["id"] == session
        && meta["session_id"].as_str().is_none_or(|id| id == session)
        && meta["cwd"]
            .as_str()
            .is_some_and(|path| Path::new(path) == cwd)
        && meta["source"] == "cli"
        && meta["originator"] == "codex-tui"
        && meta["model_provider"] == "openai"
}

#[cfg(test)]
#[path = "../../tests/observer_unit/native_receipt_binding.rs"]
mod tests;
