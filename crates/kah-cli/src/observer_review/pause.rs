//! A pause is expected control state, not a recurring observer error.
use crate::Result;
use serde_json::{Value, json};
use std::{fs::OpenOptions, os::unix::fs::OpenOptionsExt, path::Path};

pub fn notice(directory: &Path, marker: &Path) -> Result<Option<bool>> {
    kah_core::run_store::ensure_private_directory(directory)?;
    let lock = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW)
        .open(directory.join("pause-notice.lock"))?;
    match lock.try_lock() {
        Ok(()) => (),
        Err(std::fs::TryLockError::WouldBlock) => return Ok(None),
        Err(std::fs::TryLockError::Error(error)) => return Err(error.into()),
    }
    let paused = marker.is_file();
    let previous = super::state::read::<Value>(&directory.join("pause-notice.json"))?
        .is_some_and(|s| s["paused"] == true);
    if paused == previous {
        return Ok(None);
    }
    crate::unattended_report::save(directory, "pause-notice.json", &json!({"paused":paused}))?;
    Ok(Some(paused))
}

pub fn append(output: &mut Value, paused: bool) {
    let text = if paused {
        "kah observer paused; no review inference will run until resumed."
    } else {
        "kah observer resumed."
    };
    let previous = output["systemMessage"].as_str().unwrap_or("");
    output["systemMessage"] = if previous.is_empty() {
        text.into()
    } else {
        format!("{previous}\n{text}").into()
    };
}

pub fn annotate(input: &Value, output: &mut Value) -> Result<()> {
    if input["agent_id"].is_string() {
        return Ok(());
    }
    let Some(worker) = input["session_id"].as_str() else {
        return Ok(());
    };
    let base = kah_core::run_store::RunStore::default_state_root()?;
    let session = kah_observer::session::Store::directory(&base, worker);
    if !session.is_dir() {
        return Ok(());
    }
    if let Some(paused) = notice(&session.join("review"), &session.join("observer-paused"))? {
        append(output, paused);
    }
    Ok(())
}
