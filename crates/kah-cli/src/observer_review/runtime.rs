use super::{
    capture, control, launch,
    state::{Pending, Purpose, State, Store},
    workspace,
};
use crate::{Result, relay_transport::herdr};
use serde_json::{Value, json};
use std::{
    env,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

/// Called from a bounded detached host job, never synchronously by a tool hook.
pub fn wake(worker: &str, purpose: Purpose, turn: &str, trigger: Value) -> Result<Value> {
    require_active(worker)?;
    super::queue::enqueue(worker, purpose, turn, trigger)?;
    super::queue::drain(worker)
}
pub(super) fn submit(worker: &str, purpose: Purpose, turn: &str, trigger: Value) -> Result<Value> {
    require_active(worker)?;
    let started = kah_observer::session::now_ms();
    let home = PathBuf::from(env::var_os("HOME").ok_or("REVIEW_HOME stage=wake")?);
    let store = Store::open(worker)?;
    let state = match store.load()? {
        Some(state) => state,
        None => State::new(crate::relay_store::endpoint(worker)?, &home)?,
    };
    if state.stopped && state.pending.is_some() {
        drop(store);
        resume(worker)?;
        let state = Store::read(worker)?.ok_or("REVIEW_STATE_MISSING stage=resume")?;
        return Ok(
            json!({"status":"resumed_pending","pending":state.pending,"pane":state.observer.pane}),
        );
    }
    if state.pending.is_some() {
        return Ok(json!({"status":"queued","pending":state.pending}));
    }
    transport(&state)?;
    store.save(&state)?;
    workspace::prepare(&home, &state)?;
    let mut request = capture::request(&state, purpose, turn, trigger)?;
    request.created_ms = started;
    submit_snapshot(store, state, &request, &home)
}

/// Host-owned immutable snapshots use the same native review lifecycle, whether
/// captured from a live worker or reconstructed for a provenance-linked replay.
pub fn submit_snapshot(
    store: Store,
    mut state: State,
    request: &super::state::Request,
    home: &Path,
) -> Result<Value> {
    require_active(&state.worker.session)?;
    if state.pending.is_some() || request.worker != state.worker.session {
        return Err("REVIEW_SNAPSHOT_BINDING stage=review".into());
    }
    workspace::prepare(home, &state)?;
    let worker = request.worker.as_str();
    let prompt = workspace::publish(&store, &state, request)?;
    let mut pending = Pending::from(request);
    pending.native_tokens_before = if state.observer.session.is_none() {
        Some(0)
    } else {
        super::consume::tokens(home, &store, &state).ok()
    };
    state.pending = Some(pending);
    if request.purpose == Purpose::Preparation {
        return begin_preparation(store, state, request);
    }
    state.wakes += 1;
    state.stopped = false;
    let existing = state.observer.pane.is_some();
    if existing {
        control::owned_pane(&state)?;
    } else {
        state.worker = crate::relay_store::endpoint(worker)?;
        state.observer.pane = Some(create(&state)?);
        state.observer.pid = None;
        state.observer.process_stamp = None;
    }
    store.save(&state)?;
    drop(store); // SessionStart must be able to bind before pane run returns.
    deliver(home, &state, &prompt, existing)?;
    super::lifecycle::start(worker)?;
    if !existing {
        startup_notice(&state)?;
    }
    Ok(json!({"request":request.id,"status":"submitted","pane":state.observer.pane}))
}
fn create(state: &State) -> Result<String> {
    let (socket, worker_pane) = transport(state)?;
    let response = herdr(
        socket,
        &[
            "pane",
            "split",
            worker_pane,
            "--direction",
            "down",
            "--cwd",
            state
                .observer
                .workspace
                .to_str()
                .ok_or("REVIEW_PATH_UTF8 stage=launch")?,
            "--no-focus",
        ],
    )?;
    let pane = response["result"]["pane"]["pane_id"]
        .as_str()
        .ok_or("REVIEW_PANE_ID stage=launch")?
        .to_owned();
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        let view = herdr(socket, &["pane", "get", &pane])?;
        if view["result"]["pane"]["foreground_cwd"].is_string() {
            return Ok(pane);
        }
        if Instant::now() >= deadline {
            return Err("REVIEW_SHELL_TIMEOUT stage=launch".into());
        }
        std::thread::sleep(Duration::from_millis(100));
    }
}
fn deliver(home: &Path, state: &State, prompt: &str, existing: bool) -> Result<()> {
    require_active(&state.worker.session)?;
    let (socket, _) = transport(state)?;
    let pane = state
        .observer
        .pane
        .as_deref()
        .ok_or("REVIEW_PANE_ID stage=launch")?;
    if existing {
        super::delivery::warm(state, prompt)?;
    } else {
        let command = launch::command(home, state, prompt)?;
        require_active(&state.worker.session)?;
        crate::run_pane::invoke_at(
            &[
                "pane",
                "run",
                pane,
                &crate::native_run::shell_command(&command)?,
            ],
            Some(socket),
        )?;
        super::delivery::cold(state, prompt)?;
    }
    Ok(())
}
pub fn prepare_context(worker: &str) -> Result<()> {
    require_active(worker)?;
    let home = PathBuf::from(env::var_os("HOME").ok_or("REVIEW_HOME stage=context_preparation")?);
    let store = Store::open(worker)?;
    let mut state = store
        .load()?
        .ok_or("PREPARATION_STATE stage=context_preparation")?;
    let updated = helper_changed(&state)?;
    let review = state
        .pending
        .as_ref()
        .ok_or("PREPARATION_PENDING stage=context_preparation")?
        .id
        .clone();
    let decision = super::preparation::active_decision(worker, &review)?;
    let sent = store
        .directory
        .join(format!("preparation-woken-{decision}.json"));
    if sent.exists() && !updated {
        return Ok(());
    }
    workspace::prepare(&home, &state)?;
    let prompt = super::preparation::prompt(worker)?;
    let existing = state.observer.pane.is_some();
    if existing {
        control::owned_pane(&state)?;
    } else {
        state.worker = crate::relay_store::endpoint(worker)?;
        state.observer.pane = Some(create(&state)?);
        state.observer.pid = None;
        state.observer.process_stamp = None;
    }
    state.stopped = false;
    state.wakes += 1;
    store.save(&state)?;
    crate::unattended_report::save(
        &store.directory,
        sent.file_name().unwrap().to_str().unwrap(),
        &json!({"decision":decision}),
    )?;
    drop(store);
    if let Err(error) = deliver(&home, &state, &prompt, existing) {
        let _ = std::fs::remove_file(&sent);
        return Err(error);
    }
    super::lifecycle::start(worker)?;
    if !existing {
        startup_notice(&state)?;
    }
    Ok(())
}

pub(super) fn helper_changed(state: &State) -> Result<bool> {
    let home = PathBuf::from(env::var_os("HOME").ok_or("REVIEW_HOME stage=helper")?);
    Ok(std::fs::read(state.observer.workspace.join("bin/kah")).ok()
        != Some(std::fs::read(home.join(".local/bin/kah"))?))
}
pub(super) fn require_active(worker: &str) -> Result<()> {
    if kah_observer::session::paused(
        &kah_core::run_store::RunStore::default_state_root()?,
        worker,
    ) {
        return Err("REVIEW_PAUSED stage=dispatch".into());
    }
    Ok(())
}
pub fn transport(state: &State) -> Result<(&Path, &str)> {
    Ok((
        state
            .worker
            .socket
            .as_deref()
            .ok_or("REVIEW_HERDR_REQUIRED stage=observer")?,
        state
            .worker
            .pane
            .as_deref()
            .ok_or("REVIEW_WORKER_PANE stage=observer")?,
    ))
}

pub fn resume(worker: &str) -> Result<()> {
    let home = PathBuf::from(env::var_os("HOME").ok_or("REVIEW_HOME stage=resume")?);
    let store = Store::open(worker)?;
    let Some(mut state) = store.load()? else {
        return Ok(());
    };
    if !state.stopped || state.observer.pane.is_some() {
        return Ok(());
    }
    state.worker = crate::relay_store::endpoint(worker)?;
    workspace::prepare(&home, &state)?;
    let prompt = if let Some(pending) = &state.pending {
        let request = workspace::request(&store, &pending.id)?;
        if super::preparation::pending(worker, &pending.id)? {
            super::preparation::prompt(worker)?
        } else {
            workspace::prompt(
                &state,
                &request,
                &state.observer.workspace.join("inputs").join(&pending.id),
            )?
        }
    } else {
        String::new()
    };
    state.observer.pane = Some(create(&state)?);
    state.observer.pid = None;
    state.observer.process_stamp = None;
    state.stopped = false;
    store.save(&state)?;
    drop(store);
    deliver(&home, &state, &prompt, false)?;
    super::lifecycle::start(worker)?;
    startup_notice(&state)
}

pub fn retry_pending(worker: &str) -> Result<()> {
    require_active(worker)?;
    let store = Store::open(worker)?;
    let Some(state) = store.load()? else {
        return Ok(());
    };
    let Some(pending) = &state.pending else {
        return Ok(());
    };
    let gap_path = store.directory.join("gap.json");
    let gap: Option<Value> = super::state::read(&gap_path)?;
    if !gap.as_ref().is_some_and(|g| {
        g["error"].as_str().is_some_and(|s| {
            s.starts_with("REVIEW_REPORT_MISSING")
                || s.starts_with("REVIEW_REMINDER_MISSING")
                || s.starts_with("REVIEW_REMINDER_NATIVE_LIMIT")
                || s.starts_with("REVIEW_NOT_IDLE")
                || s.starts_with("REVIEW_SUBMISSION_UNCONFIRMED")
        })
    }) {
        return Ok(());
    }
    let request = workspace::request(&store, &pending.id)?;
    let prompt = workspace::prompt(
        &state,
        &request,
        &state.observer.workspace.join("inputs").join(&pending.id),
    )?;
    let home = PathBuf::from(env::var_os("HOME").ok_or("REVIEW_HOME stage=retry")?);
    let number = super::redelivery::begin(
        &store,
        &request.id,
        gap.as_ref().unwrap()["error"]
            .as_str()
            .unwrap_or("missing evidence report"),
    )?;
    drop(store);
    let delivery = deliver(&home, &state, &prompt, true);
    let store = Store::open(worker)?;
    super::redelivery::outcome(
        &store,
        &request.id,
        number,
        delivery.as_ref().err().map(ToString::to_string),
    )?;
    delivery?;
    if super::state::read::<Value>(&gap_path)? == gap {
        std::fs::remove_file(gap_path)?;
    }
    Ok(())
}

fn startup_notice(state: &State) -> Result<()> {
    let (socket, _) = transport(state)?;
    let pane = state
        .observer
        .pane
        .as_deref()
        .ok_or("REVIEW_PANE_ID stage=startup")?;
    let started = Instant::now();
    loop {
        let view = herdr(socket, &["pane", "get", pane])?["result"]["pane"].clone();
        let ready = Store::read(&state.worker.session)?
            .is_some_and(|s| s.observer.pid.is_some() && s.observer.session.is_some())
            || state.observer.session.is_some() && view["agent"] == state.worker.provider;
        if let Some(reason) =
            crate::run_pane::waiting_reason(&view, ready, started.elapsed().as_secs())
        {
            crate::run_pane::invoke_at(
                &[
                    "notification",
                    "show",
                    "kah observer needs attention",
                    "--body",
                    &format!(
                        "{}: {reason}. Open {} or run kah observer {}.",
                        state.worker.session, state.observer.name, state.worker.session
                    ),
                    "--sound",
                    "request",
                ],
                Some(socket),
            )?;
            return Ok(());
        }
        if ready || started.elapsed() >= Duration::from_secs(15) {
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(200));
    }
}

fn begin_preparation(store: Store, state: State, request: &super::state::Request) -> Result<Value> {
    let packet = &request.trigger["preparation"];
    let questions = serde_json::from_value(packet["questions"].clone())?;
    super::preparation::stage(
        request,
        &packet["state"],
        &questions,
        packet["reason"].as_str().unwrap_or("token planning"),
    )?;
    store.save(&state)?;
    drop(store);
    prepare_context(&request.worker)?;
    Ok(json!({"request":request.id,"status":"preparation_submitted"}))
}
