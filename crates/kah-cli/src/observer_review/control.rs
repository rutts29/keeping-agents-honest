use super::state::{self, State, Store};
use crate::Result;
use kah_observer::session::Binding;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{env, fs, path::PathBuf};

#[derive(Clone, Serialize, Deserialize)]
pub struct Config {
    pub enabled: bool,
    pub provider: String,
    pub worker: String,
    pub turn: String,
    pub pid: Option<u32>,
    pub process_stamp: Option<String>,
}
pub fn register(binding: &Binding, input: &Value, provider: &str) -> Result<()> {
    let directory = Store::directory(&binding.session)?;
    let old: Option<Config> = state::read(&directory.join("config.json"))?;
    let turn = input["turn_id"]
        .as_str()
        .or_else(|| input["prompt_id"].as_str())
        .map(str::to_owned)
        .or_else(|| {
            (input["hook_event_name"] == "UserPromptSubmit")
                .then(|| format!("prompt-{}", kah_observer::session::now_ms()))
        })
        .or_else(|| old.as_ref().map(|c| c.turn.clone()))
        .unwrap_or_else(|| "startup".into());
    let pid = native_parent(provider);
    let config = Config {
        enabled: enabled(binding)?,
        provider: provider.into(),
        worker: binding.session.clone(),
        turn,
        pid,
        process_stamp: pid.and_then(kah_observer::session::process_stamp),
    };
    if old.as_ref().is_some_and(|c| {
        c.turn == config.turn && c.pid == config.pid && c.enabled == config.enabled
    }) {
        return Ok(());
    }
    kah_core::run_store::ensure_private_directory(&directory)?;
    crate::unattended_report::save(&directory, "config.json", &serde_json::to_value(config)?)
}
pub(crate) fn enabled(binding: &Binding) -> Result<bool> {
    let home = PathBuf::from(env::var_os("HOME").ok_or("REVIEW_HOME stage=scope")?);
    if binding.project.starts_with(home.join("repos/untrusted")) {
        return Ok(false);
    }
    if binding.project.starts_with(home.join("repos/mine"))
        || binding.project.starts_with(home.join("repos/research"))
    {
        return Ok(true);
    }
    let Some(id) = binding
        .project
        .parent()
        .and_then(|p| p.file_name())
        .and_then(|p| p.to_str())
        .filter(|s| s.starts_with("run-"))
    else {
        return Ok(false);
    };
    let record = kah_run::store::Store::read(
        &kah_core::run_store::RunStore::default_state_root()?.join("runs"),
        id,
    )?;
    Ok(record.worktree == binding.project && record.domain != "untrusted")
}
pub fn native_parent(provider: &str) -> Option<u32> {
    let mut pid = std::process::id();
    for _ in 0..12 {
        let status = fs::read_to_string(format!("/proc/{pid}/status")).ok()?;
        pid = status
            .lines()
            .find_map(|s| s.strip_prefix("PPid:"))
            .and_then(|s| s.trim().parse().ok())?;
        if pid <= 1 {
            return None;
        }
        let argv = fs::read(format!("/proc/{pid}/cmdline")).ok()?;
        let first = argv.split(|b| *b == 0).next()?;
        let text = String::from_utf8_lossy(first);
        if std::path::Path::new(text.as_ref())
            .file_name()
            .is_some_and(|p| p == provider)
        {
            return Some(pid);
        }
    }
    None
}
pub fn config(worker: &str) -> Result<Option<Config>> {
    state::read(&Store::directory(worker)?.join("config.json"))
}
pub fn focus(worker: &str) -> Result<()> {
    let state = Store::read(worker)?
        .ok_or("REVIEW_NOT_STARTED stage=observer: no tier-2/3 event has created this observer")?;
    owned_pane(&state)?;
    let pane = state
        .observer
        .pane
        .as_deref()
        .ok_or("REVIEW_STOPPED stage=observer: resume the worker first")?;
    let socket = state
        .worker
        .socket
        .as_deref()
        .ok_or("REVIEW_HERDR_REQUIRED stage=observer")?;
    crate::relay_transport::herdr(socket, &["agent", "focus", pane])?;
    println!(
        "{}",
        json!({"worker":worker,"observer":state.observer.session,"pane":pane})
    );
    Ok(())
}
pub fn status(worker: &str) -> Result<Value> {
    let intent = intent_status(worker)?;
    let gap: Option<Value> = state::read(&Store::directory(worker)?.join("gap.json"))?;
    let liveness: Option<Value> =
        state::read(&Store::directory(worker)?.join("liveness/progress.json"))?;
    let questions = super::questions::pending(&Store::directory(worker)?)?;
    let queued = super::queue::entries(worker)?
        .into_iter()
        .map(|(path, wake)| json!({"record":path,"purpose":wake.purpose,"turn":wake.turn}))
        .collect::<Vec<_>>();
    let checkpoint_events_pending = super::events::pending(worker)?;
    let mut value = Store::read(worker)?.map(|s|json!({"session":s.observer.session,"pane":s.observer.pane,"model":s.observer.model,"effort":s.observer.effort,"wakes":s.wakes,"pending":s.pending,"latest":s.latest,"deferrals":s.deferrals,"questions":questions,"gap":gap})).unwrap_or_else(||gap.map(|g|json!({"gap":g,"questions":questions})).unwrap_or_else(||if questions.is_empty(){Value::Null}else{json!({"questions":questions})}));
    if !value.is_null()
        || !queued.is_empty()
        || checkpoint_events_pending
        || liveness.is_some()
        || intent.is_some()
    {
        if value.is_null() {
            value = json!({});
        }
        value["queued"] = json!(queued);
        value["admission"] = json!(state::read::<Value>(
            &Store::directory(worker)?.join("admission-blocked.json")
        )?);
        value["scheduling"] = super::queue::status(worker)?;
        value["checkpoint_events_pending"] = json!(checkpoint_events_pending);
        value["service"] = json!(state::read::<Value>(
            &Store::directory(worker)?.join("service-outage.json")
        )?);
        if let Some(intent) = intent {
            value["intent_status"] = intent;
        }
        if let Some(progress) = liveness {
            value["liveness"] = json!({"progress":progress,"journal":Store::directory(worker)?.join("liveness/events.jsonl")});
        }
    }
    Ok(value)
}
fn intent_status(worker: &str) -> Result<Option<Value>> {
    let Some(source) = kah_observer::session::Store::read(
        &kah_core::run_store::RunStore::default_state_root()?,
        worker,
    )?
    else {
        return Ok(None);
    };
    let active = kah_observer::engine::projection_wait(&source)?
        .is_some_and(|p| !p.goal.text.trim().is_empty());
    Ok(Some(
        json!({"active":active,"message":if active {"observer intent is active"} else {"observer has no intent to check"}}),
    ))
}

pub fn owned_pane(state: &State) -> Result<crate::run_pane::Pane> {
    let pane = state
        .observer
        .pane
        .as_deref()
        .ok_or("REVIEW_PANE_MISSING stage=observer")?;
    let socket = state
        .worker
        .socket
        .as_deref()
        .ok_or("REVIEW_HERDR_REQUIRED stage=observer")?;
    let view = crate::relay_transport::herdr(socket, &["pane", "get", pane])?;
    let view = &view["result"]["pane"];
    if view["agent"] != state.worker.provider
        || !view["foreground_cwd"]
            .as_str()
            .is_some_and(|p| contains_workspace(&state.observer.workspace, p))
        || state.observer.pid.is_some_and(|pid| {
            kah_observer::session::process_stamp(pid) != state.observer.process_stamp
        })
    {
        return Err("REVIEW_PANE_REPLACED stage=observer".into());
    }
    Ok(crate::run_pane::Pane { id: pane.into() })
}
pub fn contains_workspace(root: &std::path::Path, cwd: &str) -> bool {
    PathBuf::from(cwd)
        .canonicalize()
        .is_ok_and(|p| p.starts_with(root))
}
