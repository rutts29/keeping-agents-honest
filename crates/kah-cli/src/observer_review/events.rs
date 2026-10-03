//! Durable checkpoint events are independent of the long-running review lock.
use super::{
    hook, runtime,
    state::{Purpose, Store},
};
use crate::Result;
use serde_json::{Value, json};
use std::{env, fs, path::PathBuf};

pub fn enqueue(worker: &str, event: Value) -> Result<()> {
    runtime::require_active(worker)?;
    let directory = Store::directory(worker)?.join("checkpoint-events");
    kah_core::run_store::ensure_private_directory(&directory)?;
    crate::unattended_report::save(
        &directory,
        &format!("{}.json", crate::relay_store::nonce()),
        &event,
    )?;
    hook::spawn("__checkpoint-worker", worker)
}
pub fn pending(worker: &str) -> Result<bool> {
    let directory = Store::directory(worker)?.join("checkpoint-events");
    Ok(directory.exists() && fs::read_dir(directory)?.next().is_some())
}
pub fn run(worker: &str) -> Result<()> {
    if Store::read(worker)?.is_some_and(|s| s.pending.is_some()) {
        return Ok(());
    }
    let directory = Store::directory(worker)?.join("checkpoint-events");
    if !directory.exists() {
        return Ok(());
    }
    let mut paths = fs::read_dir(&directory)?
        .map(|e| e.map(|e| e.path()))
        .collect::<std::io::Result<Vec<_>>>()?;
    paths.sort();
    if paths.is_empty() {
        return Ok(());
    }
    let events = paths
        .iter()
        .map(|p| {
            super::state::read::<Value>(p)?
                .ok_or_else(|| "REVIEW_EVENT_MISSING stage=checkpoint".into())
        })
        .collect::<Result<Vec<_>>>()?;
    let turn = super::control::config(worker)?
        .map(|c| c.turn)
        .unwrap_or_else(|| "checkpoint".into());
    let receipt = runtime::wake(worker, Purpose::Checkpoint, &turn, json!({"events":events}))?;
    if receipt["status"] == "submitted" {
        for path in paths {
            fs::remove_file(path)?;
        }
    }
    Ok(())
}
pub fn cli(args: &[String]) -> Result<()> {
    if args.len() > 2 {
        return Err("CHECKPOINT_ARGUMENTS stage=checkpoint: kah gate [SESSION]".into());
    }
    let worker = if let Some(id) = args.get(1) {
        id.clone()
    } else {
        current()?
    };
    let config =
        super::control::config(&worker)?.ok_or("CHECKPOINT_WORKER_UNKNOWN stage=checkpoint")?;
    if !config.enabled {
        return Err(
            "CHECKPOINT_DOMAIN stage=checkpoint: full reviews apply to Mine and Research".into(),
        );
    }
    enqueue(
        &worker,
        json!({"kind":"user_gate","source":"host_cli","at_ms":kah_observer::session::now_ms()}),
    )?;
    println!("{}", json!({"worker":worker,"checkpoint":"queued"}));
    Ok(())
}
fn current() -> Result<String> {
    if let Ok(id) = env::var("CODEX_THREAD_ID") {
        return Ok(id);
    }
    if let (Ok(pane), Some(socket)) = (env::var("HERDR_PANE_ID"), env::var_os("HERDR_SOCKET_PATH"))
        && let Ok(endpoint) = crate::relay_store::pane_endpoint(&pane, &PathBuf::from(socket))
    {
        return Ok(endpoint.session);
    }
    let cwd = env::current_dir()?.canonicalize()?;
    let sessions = crate::inspection_cli::load_sessions(
        &kah_core::run_store::RunStore::default_state_root()?,
        None,
    )?;
    let active = sessions
        .into_iter()
        .filter(|s| cwd.starts_with(&s.binding.project) && s.run_id.is_some())
        .filter_map(|s| super::control::config(&s.binding.session).ok().flatten())
        .filter(|c| {
            c.enabled
                && c.pid
                    .is_some_and(|p| kah_observer::session::process_stamp(p) == c.process_stamp)
        })
        .collect::<Vec<_>>();
    if active.len() != 1 {
        return Err("CHECKPOINT_SESSION_REQUIRED stage=checkpoint: select one active kah session with kah gate SESSION".into());
    }
    Ok(active[0].worker.clone())
}
