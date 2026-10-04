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
    std::io::stdin().read_to_end(&mut bytes)?;
    let input: Value = serde_json::from_slice(&bytes)?;
    if input["agent_id"].is_string() {
        return Ok(());
    }
    let provider = Store::read(worker)?
        .ok_or("REVIEW_STATE_MISSING stage=hook")?
        .worker
        .provider;
    let event = super::hook_queue::Event::capture(input, super::control::native_parent(&provider));
    match apply(worker, &event) {
        Err(e) if e.to_string().starts_with("REVIEW_BUSY") => {
            super::hook_queue::enqueue(worker, &event)
        }
        other => other,
    }
}
pub(super) fn apply(worker: &str, event: &super::hook_queue::Event) -> Result<()> {
    apply_store(worker, event, Store::open(worker)?)
}
pub(super) fn apply_wait(worker: &str, event: &super::hook_queue::Event) -> Result<()> {
    apply_store(worker, event, Store::open_wait(worker)?)
}
fn apply_store(worker: &str, event: &super::hook_queue::Event, store: Store) -> Result<()> {
    let input = &event.input;
    let mut state = store.load()?.ok_or("REVIEW_STATE_MISSING stage=hook")?;
    validate(&state, event)?;
    let events = store.directory.join("hook-observations.jsonl");
    use std::io::Write;
    let mut journal = OpenOptions::new()
        .create(true)
        .append(true)
        .mode(0o600)
        .open(events)?;
    writeln!(
        journal,
        "{}",
        json!({"event":input["hook_event_name"],"session":input["session_id"],"pid":event.pid,"at_ms":kah_observer::session::now_ms()})
    )?;
    journal.sync_all()?;
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
    state.observer.pid = event.pid;
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
    if matches!(event, "Stop" | "PostToolUse")
        && let Some(pending) = &state.pending
    {
        let decision = super::preparation::active_decision(worker, &pending.id)?;
        if super::preparation::pending(worker, &decision)? {
            if state
                .observer
                .workspace
                .join("outbox")
                .join(format!("selected-{decision}.json"))
                .exists()
            {
                spawn("__review-consume", worker)?;
            } else if event == "Stop" {
                gap(
                    worker,
                    &format!(
                        "REVIEW_PREPARATION_MISSING stage=observer: native turn ended without an acknowledged selection for {decision}; request retained"
                    ),
                )?;
            }
            return Ok(());
        }
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
        } else if event == "Stop" {
            gap(
                worker,
                &"REVIEW_REPORT_MISSING stage=observer: native turn ended without a submitted evidence report; focus the observer to inspect it",
            )?;
        }
    }
    Ok(())
}
fn validate(state: &State, event: &super::hook_queue::Event) -> Result<()> {
    let input = &event.input;
    if !input["cwd"]
        .as_str()
        .is_some_and(|p| super::control::contains_workspace(&state.observer.workspace, p))
        || event.pane.as_deref() != state.observer.pane.as_deref()
        || event.socket != state.worker.socket
    {
        return Err("REVIEW_HOOK_SCOPE stage=hook".into());
    }
    Ok(())
}
pub fn spawn(command: &str, worker: &str) -> Result<()> {
    if command == "__review-wake" {
        return spawn_command(command, worker, Stdio::null());
    }
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
    spawn_command(command, worker, Stdio::from(lock))
}
fn spawn_command(command: &str, worker: &str, stdin: Stdio) -> Result<()> {
    let log = OpenOptions::new()
        .create(true)
        .append(true)
        .mode(0o600)
        .open(Store::directory(worker)?.join(format!("job-{command}.log")))?;
    Command::new(env::current_exe()?)
        .args([command, worker])
        .stdin(stdin)
        .stdout(Stdio::null())
        .stderr(Stdio::from(log))
        .process_group(0)
        .spawn()?;
    Ok(())
}
pub fn gap(worker: &str, error: &dyn std::fmt::Display) -> Result<()> {
    if error.to_string().starts_with("REVIEW_PAUSED")
        || error.to_string().starts_with("REVIEW_BUSY")
    {
        return Ok(());
    }
    let store = Store::open(worker)?;
    crate::unattended_report::save(
        &store.directory,
        "gap.json",
        &json!({"error":if error.to_string().contains(" stage=") { error.to_string() } else {format!("REVIEW_IO stage=review: {error}")},"at_ms":kah_observer::session::now_ms()}),
    )
}
