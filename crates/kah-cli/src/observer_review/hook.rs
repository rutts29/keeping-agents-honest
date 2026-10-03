use super::{
    runtime,
    state::{State, Store},
};
use crate::Result;
use serde_json::{Value, json};
use std::{
    env,
    fs::{self, OpenOptions},
    io::Read,
    os::unix::{fs::OpenOptionsExt, process::CommandExt},
    path::PathBuf,
    process::{Command, Stdio},
};

pub fn run(worker: &str) {
    let result = receive(worker);
    if let Err(error) = result
        && !error.to_string().starts_with("REVIEW_PAUSED")
    {
        eprintln!("kah observer evidence gap: {error}");
        if !error.to_string().starts_with("REVIEW_BUSY") {
            let _ = gap(worker, &error);
        }
    }
    println!("{{}}");
}
fn receive(worker: &str) -> Result<()> {
    let mut bytes = Vec::new();
    std::io::stdin()
        .take(1024 * 1024 + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() > 1024 * 1024 {
        return Err("REVIEW_HOOK_BOUND stage=hook".into());
    }
    let input: Value = serde_json::from_slice(&bytes)?;
    if input["agent_id"].is_string() {
        return Ok(());
    }
    let store = Store::open(worker)?;
    let mut state = store.load()?.ok_or("REVIEW_STATE_MISSING stage=hook")?;
    validate(&state, &input)?;
    let session = input["session_id"]
        .as_str()
        .ok_or("REVIEW_SESSION_ID stage=hook")?;
    if let Some(expected) = &state.observer.session
        && expected != session
    {
        // A user-created replacement conversation is no longer kah's pane to
        // prompt or close. Preserve the original native ID for worker resume.
        state.observer.pane = None;
        state.stopped = true;
        store.save(&state)?;
        return Err("REVIEW_SESSION_MISMATCH stage=hook".into());
    }
    state.observer.session = Some(session.into());
    state.observer.pid = super::control::native_parent(&state.worker.provider);
    state.observer.process_stamp = state
        .observer
        .pid
        .and_then(kah_observer::session::process_stamp);
    if let Some(path) = input["transcript_path"].as_str() {
        state.observer.transcript = Some(PathBuf::from(path));
    }
    let event = input["hook_event_name"].as_str().unwrap_or("");
    if event == "SessionEnd" {
        state.observer.pane = None;
        state.stopped = true;
    }
    store.save(&state)?;
    drop(store);
    handle_event(worker, &state, event)
}
fn handle_event(worker: &str, state: &State, event: &str) -> Result<()> {
    if event == "SessionStart" {
        let (socket, _) = runtime::transport(state)?;
        crate::relay_transport::herdr(
            socket,
            &[
                "agent",
                "rename",
                state.observer.pane.as_deref().unwrap(),
                &state.observer.name,
            ],
        )?;
    }
    if event == "Stop"
        && let Some(pending) = &state.pending
    {
        if state
            .observer
            .workspace
            .join("outbox")
            .join(format!("submitted-{}.json", pending.id))
            .exists()
        {
            let store = Store::open(worker)?;
            crate::unattended_report::save(
                &store.directory,
                "report-ready.json",
                &json!({"request":pending.id}),
            )?;
            drop(store);
            spawn("__review-consume", worker)?;
        } else {
            gap(
                worker,
                &"REVIEW_REPORT_MISSING stage=observer: native turn ended without a submitted evidence report; focus the observer to inspect it",
            )?;
        }
    }
    Ok(())
}
fn validate(state: &State, input: &Value) -> Result<()> {
    if !input["cwd"]
        .as_str()
        .is_some_and(|p| super::control::contains_workspace(&state.observer.workspace, p))
        || env::var("HERDR_PANE_ID").ok().as_deref() != state.observer.pane.as_deref()
        || env::var_os("HERDR_SOCKET_PATH").map(PathBuf::from) != state.worker.socket
    {
        return Err("REVIEW_HOOK_SCOPE stage=hook".into());
    }
    Ok(())
}
pub fn spawn(command: &str, worker: &str) -> Result<()> {
    let lock = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .mode(0o600)
        .open(Store::directory(worker)?.join(format!("job-{command}.lock")))?;
    match lock.try_lock() {
        Ok(()) => (),
        Err(fs::TryLockError::WouldBlock) => return Ok(()),
        Err(e) => return Err(e.into()),
    }
    Command::new("/usr/bin/timeout")
        .args(["--kill-after=2s", "90s"])
        .arg(env::current_exe()?)
        .args([command, worker])
        .stdin(Stdio::from(lock))
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .process_group(0)
        .spawn()?;
    Ok(())
}
pub fn gap(worker: &str, error: &dyn std::fmt::Display) -> Result<()> {
    if error.to_string().starts_with("REVIEW_PAUSED") {
        return Ok(());
    }
    let store = Store::open(worker)?;
    crate::unattended_report::save(
        &store.directory,
        "gap.json",
        &json!({"error":error.to_string(),"at_ms":kah_observer::session::now_ms()}),
    )
}
