//! Every admission return is recorded, including failures before a request exists.
use crate::{
    Result,
    observer_review::{
        source_store,
        state::{self, Store},
    },
};
use serde_json::{Value, json};
use std::{fs, io::Write, os::unix::fs::OpenOptionsExt, path::Path};

pub fn run(worker: &str, action: impl FnOnce() -> Result<Value>) -> Result<Value> {
    let root = Store::directory(worker)?;
    event(&root, json!({"kind":"started","worker":worker}))?;
    let started = std::time::Instant::now();
    let result = action();
    let reason = match &result {
        Ok(value) => format!(
            "REVIEW_ADMISSION_RETURN stage=admission: {}",
            value["status"].as_str().unwrap_or("unknown")
        ),
        Err(error) if error.to_string().contains(" stage=") => error.to_string(),
        Err(error) => format!("REVIEW_ADMISSION_ERROR stage=admission: {error}"),
    };
    event(
        &root,
        json!({"kind":"return","worker":worker,"success":result.is_ok(),"reason":reason,
        "elapsed_ms":started.elapsed().as_millis(),"receipt":result.as_ref().ok().map(|v|json!({"status":v["status"],"request":v["request"],"reason":v["reason"],"head":v["head"]}))}),
    )?;
    match &result {
        Err(_) => blocked(worker, &root, &reason)?,
        Ok(value)
            if matches!(
                value["status"].as_str(),
                Some("submitted" | "preparation_submitted" | "option1_reviewed" | "idle")
            ) =>
        {
            recover(&root)?
        }
        _ => (),
    }
    result
}
fn event(root: &Path, event: Value) -> Result<()> {
    kah_core::run_store::ensure_private_directory(root)?;
    let mut file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .mode(0o600)
        .open(root.join("admission-outcomes.jsonl"))?;
    writeln!(
        file,
        "{}",
        json!({"at_ms":kah_observer::session::now_ms(),"pid":std::process::id(),"event":event})
    )?;
    file.sync_all()?;
    Ok(())
}
fn blocked(worker: &str, root: &Path, reason: &str) -> Result<()> {
    if super::super::failure::pending(&reason)
        || reason.starts_with("REVIEW_BUSY")
        || reason.starts_with("REVIEW_PAUSED")
    {
        return Ok(());
    }
    let mut queued = if root.join("queued").is_dir() {
        fs::read_dir(root.join("queued"))?
            .map(|e| e.map(|e| e.file_name().to_string_lossy().into_owned()))
            .collect::<std::io::Result<Vec<_>>>()?
    } else {
        vec![]
    };
    queued.sort();
    let identity = json!({"worker":worker,"error":reason,"queued":queued});
    let old: Option<Value> = state::read(&root.join("admission-blocked.json"))?;
    let text = format!(
        "[kah-observer-admission-blocked] Worker {worker}: {reason}; queued triggers retained. Host admission needs recovery; no report correction requested."
    );
    // at_ms is the latest failure: completion_checkpoint stops a run when a
    // retry fails again. The reviewer is told once per identity.
    crate::unattended_report::save(
        root,
        "admission-blocked.json",
        &json!({"identity":identity,
        "worker":worker,"at_ms":kah_observer::session::now_ms(),"error":reason,"text":text,"systemMessage":text}),
    )?;
    if old.as_ref().is_some_and(|v| v["identity"] == identity) {
        return Ok(());
    }
    let sent = state::read_control(worker)?
        .and_then(|s| s.worker.socket)
        .map(|socket| {
            crate::run_pane::invoke_at(
                &[
                    "agent",
                    "prompt",
                    crate::reviewer_agent::reviewer_agent(),
                    &format!("[kah-builder] {text}"),
                ],
                Some(&socket),
            )
        });
    crate::unattended_report::save(
        root,
        "admission-reviewer-notice.json",
        &json!({"identity":identity,
        "submitted":sent.as_ref().is_some_and(|r|r.is_ok()),"error":sent.and_then(|r|r.err()).map(|e|e.to_string())}),
    )
}
fn recover(root: &Path) -> Result<()> {
    let paths = [
        "admission-blocked.json",
        "admission-hook-notice.json",
        "admission-reviewer-notice.json",
    ];
    if !paths.iter().any(|p| root.join(p).is_file()) {
        return Ok(());
    }
    let archive = root
        .join("admission-recoveries")
        .join(crate::relay_store::nonce());
    kah_core::run_store::ensure_private_directory(&archive)?;
    for name in paths {
        if root.join(name).is_file() {
            source_store::access_repair::archive(&root.join(name), &archive.join(name))?;
        }
    }
    Ok(())
}
pub fn notice(worker: &str) -> Result<Option<Value>> {
    let root = Store::directory(worker)?;
    let Some(block): Option<Value> = state::read(&root.join("admission-blocked.json"))? else {
        return Ok(None);
    };
    let prior: Option<Value> = state::read(&root.join("admission-hook-notice.json"))?;
    if prior.as_ref() == Some(&block["identity"]) {
        return Ok(None);
    }
    crate::unattended_report::save(&root, "admission-hook-notice.json", &block["identity"])?;
    Ok(Some(json!({"systemMessage":block["systemMessage"]})))
}
