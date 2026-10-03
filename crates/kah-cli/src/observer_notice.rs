//! Empty intent is visible control state; detecting it never runs inference.
use crate::Result;
use serde_json::{Value, json};
use std::{fs::OpenOptions, os::unix::fs::OpenOptionsExt, path::Path};

pub fn notice(directory: &Path, idle: bool) -> Result<Option<bool>> {
    kah_core::run_store::ensure_private_directory(directory)?;
    let lock = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW)
        .open(directory.join("idle-notice.lock"))
        .map_err(|e| format!("OBSERVER_NOTICE_IO stage=idle_notice: {e}"))?;
    match lock.try_lock() {
        Ok(()) => (),
        Err(std::fs::TryLockError::WouldBlock) => return Ok(None),
        Err(std::fs::TryLockError::Error(e)) => {
            return Err(format!("OBSERVER_NOTICE_LOCK stage=idle_notice: {e}").into());
        }
    }
    let previous =
        crate::observer_review::state::read::<Value>(&directory.join("idle-notice.json"))?
            .is_some_and(|value| value["idle"] == true);
    if idle == previous {
        return Ok(None);
    }
    crate::unattended_report::save(directory, "idle-notice.json", &json!({"idle":idle}))?;
    Ok(Some(idle))
}

pub fn append(output: &mut Value, idle: bool) {
    let text = if idle {
        "kah observer idle: intent ledger is empty; no intent review inference runs until user intent is recorded."
    } else {
        "kah observer active: user intent is recorded."
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
    let Some(state) = kah_observer::session::Store::read(&base, worker)? else {
        return Ok(());
    };
    let directory = kah_observer::session::Store::directory(&base, worker);
    if let Some(idle) = notice(&directory, state.run_id.is_none())? {
        append(output, idle);
    }
    Ok(())
}
