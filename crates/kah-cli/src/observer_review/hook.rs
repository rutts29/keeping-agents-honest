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
            let _ = gap_error(worker, error.as_ref());
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
    let state = Store::read(worker)?.ok_or("REVIEW_STATE_MISSING stage=hook")?;
    let mut event = super::hook_queue::Event::capture(
        input,
        super::control::native_parent(&state.worker.provider),
    );
    validate(&state, &event)?;
    event.validated_pane = event.pane.clone();
    event.transcript_end = state
        .observer
        .transcript
        .as_ref()
        .and_then(|path| fs::metadata(path).ok())
        .map(|meta| meta.len());
    // Native hooks have a short invocation timeout. Persist the validated
    // payload first; transcript scans and host handoffs belong to the job.
    super::hook_queue::enqueue(worker, &event)
}
pub(super) fn apply_wait(worker: &str, event: &super::hook_queue::Event) -> Result<()> {
    apply_store(worker, event, Store::open_wait(worker)?)
}
fn apply_store(worker: &str, event: &super::hook_queue::Event, store: Store) -> Result<()> {
    let input = &event.input;
    let mut state = store.load()?.ok_or("REVIEW_STATE_MISSING stage=hook")?;
    validate(&state, event)?;
    if state.observer.session.as_deref() == input["session_id"].as_str() {
        super::timing::native_end(worker, event)?;
    }
    if event.retired_for(&state) {
        return retain_transport_event(worker, event, store, &state);
    }
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
        json!({"event":input["hook_event_name"],"session":input["session_id"],"pid":event.pid,"request_context":event.context,"pending":state.pending.as_ref().map(|p|&p.id),"at_ms":kah_observer::session::now_ms()})
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
    let kind = input["hook_event_name"].as_str().unwrap_or("");
    if kind == "SessionEnd" {
        state.observer.pane = None;
        state.stopped = true;
    }
    store.save(&state)?;
    drop(store);
    if matches!(kind, "PostToolUse" | "Stop") && stale(worker, &state, event.context.as_ref())? {
        return Ok(());
    }
    handle_event(worker, &state, kind)
}
fn retain_transport_event(
    worker: &str,
    event: &super::hook_queue::Event,
    store: Store,
    state: &State,
) -> Result<()> {
    let retained = store.directory.join("retired-hook-events");
    kah_core::run_store::ensure_private_directory(&retained)?;
    crate::unattended_report::save(
        &retained,
        &format!("{}.json", crate::relay_store::nonce()),
        &serde_json::to_value(event)?,
    )?;
    drop(store);
    // Session identity outlives its pane. Preserve current PID/lifetime and
    // consume any ready current-request evidence, independent of hook type.
    if state.pending.is_some() {
        handle_event(worker, state, "PostToolUse")
    } else {
        Ok(())
    }
}

fn stale(
    worker: &str,
    state: &State,
    context: Option<&super::request_context::Context>,
) -> Result<bool> {
    let (Some(pending), Some(context)) = (&state.pending, context) else {
        return Ok(false);
    };
    let decision = super::preparation::active_decision(worker, &pending.id)?;
    Ok(context.request != pending.id && context.request != decision
        || super::preparation::pending(worker, &decision)? && !context.preparation)
}
fn handle_event(worker: &str, state: &State, event: &str) -> Result<()> {
    super::delivery_outcome::observe(worker)?;
    if event == "SessionStart" {
        bind_name(state)?;
    }
    if matches!(event, "Stop" | "PostToolUse")
        && let Some(pending) = &state.pending
    {
        pending_event(worker, state, event, &pending.id)?;
    }
    if state.pending.is_none() {
        // A queued wake can lose a nonblocking store race with this hook.
        // The writer has released the store; complete that handoff now.
        super::queue::kick(worker)?;
    }
    Ok(())
}
/// The observer's turn events for its pending request: consume a submitted
/// report, retry a rejected one, or handle a turn that ended without one.
fn pending_event(worker: &str, state: &State, event: &str, request: &str) -> Result<()> {
    let decision = super::preparation::active_decision(worker, request)?;
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
            record_gap(
                worker,
                &format!(
                    "REVIEW_PREPARATION_MISSING stage=observer: native turn ended without an acknowledged selection for {decision}; request retained"
                ),
                super::failure::Origin::Delivery,
            )?;
        }
        return Ok(());
    }
    if rejected_report(state, request) {
        // Reproduce this rejection in a fresh detached host job. Never rely
        // on a long-lived watcher compiled before this report protocol.
        return spawn("__review-retry", worker);
    }
    if state
        .observer
        .workspace
        .join("outbox")
        .join(format!("submitted-{}.json", request))
        .exists()
    {
        super::timing::now(&state.worker.session, request, "acknowledged")?;
        let store = Store::open(worker)?;
        crate::unattended_report::save(
            &store.directory,
            "report-ready.json",
            &json!({"request":request}),
        )?;
        drop(store);
        spawn("__review-consume", worker)?;
    } else if event == "Stop" {
        use super::missing_report::{Next, next, retire};
        if next(&Store::directory(worker)?, request)? == Next::Retire {
            return retire(worker, request);
        }
        record_gap(
            worker,
            &"REVIEW_REPORT_MISSING stage=observer: native turn ended without a submitted evidence report; focus the observer to inspect it",
            super::failure::Origin::Delivery,
        )?;
        // A fresh observer turn on the same request, once.
        spawn("__review-retry", worker)?;
    }
    Ok(())
}
fn rejected_report(state: &State, request: &str) -> bool {
    let outbox = state.observer.workspace.join("outbox");
    !outbox.join(format!("submitted-{request}.json")).is_file()
        && outbox.join(format!("rejected-{request}.json")).is_file()
}
fn bind_name(state: &State) -> Result<()> {
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
    Ok(())
}
fn validate(state: &State, event: &super::hook_queue::Event) -> Result<()> {
    let input = &event.input;
    if !input["cwd"]
        .as_str()
        .is_some_and(|p| super::control::contains_workspace(&state.observer.workspace, p))
        || (event.pane.as_deref() != state.observer.pane.as_deref()
            && !state
                .observer
                .session
                .as_deref()
                .is_some_and(|session| input["session_id"].as_str() == Some(session)))
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
    // Installation can replace our executable while this host watcher remains
    // alive. /proc/self/exe then names a deleted inode, not a launchable path.
    let home = PathBuf::from(env::var_os("HOME").ok_or("REVIEW_HOME stage=spawn")?);
    let installed = home.join(".local/bin/kah");
    let program = if installed.is_file() {
        installed
    } else {
        // A disposable home may run kah directly without installation. Only
        // a still-present kah binary is valid, never a library's test runner.
        let current = env::current_exe()?;
        if current.file_name().is_none_or(|name| name != "kah") || !current.is_file() {
            return Err(format!(
                "REVIEW_PROGRAM_MISSING stage=spawn: {}",
                installed.display()
            )
            .into());
        }
        current
    };
    Command::new(program)
        .args([command, worker])
        .stdin(stdin)
        .stdout(Stdio::null())
        .stderr(Stdio::from(log))
        .process_group(0)
        .spawn()?;
    Ok(())
}
pub fn gap(worker: &str, error: &dyn std::fmt::Display) -> Result<()> {
    record_gap(worker, error, super::failure::Origin::Host)
}
pub fn gap_error(worker: &str, error: &(dyn std::error::Error + 'static)) -> Result<()> {
    if super::delivery_outcome::reason(error) {
        return Ok(());
    }
    if super::provider_block::reason(&error) {
        super::provider_block::retain(worker, None, "native precaution screen")?;
        return Ok(());
    }
    if super::failure::pending(&error) {
        super::consume::outcome::record(
            worker,
            json!({"kind":"pending","request":super::consume::outcome::pending(worker)?,"reason":error.to_string()}),
        )?;
        return spawn("__review-kick", worker);
    }
    let mut gap = super::failure_snapshot::capture(worker, error)?;
    gap["at_ms"] = kah_observer::session::now_ms().into();
    let store = Store::open(worker)?;
    if !super::failure_snapshot::gap::save(&store, &gap)? {
        return Ok(());
    }
    drop(store);
    if kah_observer::judge::availability::outage(gap["error"].as_str().unwrap_or("")) {
        // Native hooks must not wait for a service probe or Herdr notice.
        spawn("__review-kick", worker)?;
        return Ok(());
    }
    let origin = serde_json::from_value(gap["origin"].clone()).unwrap_or_default();
    if super::failure::correctable(origin, gap["error"].as_str().unwrap_or("")) {
        // A post-ACK failure may arrive after the native Stop hook; preserve
        // the same correction handoff even when the worker remains idle.
        spawn("__review-retry", worker)?;
    }
    Ok(())
}
fn record_gap(
    worker: &str,
    error: &dyn std::fmt::Display,
    origin: super::failure::Origin,
) -> Result<()> {
    if super::delivery_outcome::reason(error) {
        return Ok(());
    }
    if super::provider_block::reason(error) {
        super::provider_block::retain(worker, None, "native precaution screen")?;
        return Ok(());
    }
    if super::failure::pending(error) {
        return spawn("__review-kick", worker);
    }
    if error.to_string().starts_with("REVIEW_PAUSED")
        || error.to_string().starts_with("REVIEW_BUSY")
    {
        return Ok(());
    }
    let store = Store::open(worker)?;
    crate::unattended_report::save(
        &store.directory,
        "gap.json",
        &json!({"error":if error.to_string().contains(" stage=") { error.to_string() } else {format!("REVIEW_IO stage=review: {error}")},"at_ms":kah_observer::session::now_ms(),"origin":origin}),
    )
}
