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
    if !directory.exists() {
        return Ok(false);
    }
    for entry in fs::read_dir(directory)? {
        if entry?.path().extension().is_some_and(|e| e == "json") {
            return Ok(true);
        }
    }
    Ok(false)
}
pub fn run(worker: &str) -> Result<()> {
    let lease = super::job_lease::lease(worker, "__checkpoint-worker")?;
    drain_all(worker, &lease, |w| super::queue::drain(w).map(|_| ()))
}
/// hook::spawn starts nothing while this job holds its lock, so an event
/// written during a drain had no job left to transfer it (5f, 2026-10-17: a
/// worker commit was never reviewed). The job settles instead of exiting.
pub fn drain_all(
    worker: &str,
    lease: &fs::File,
    mut drain: impl FnMut(&str) -> Result<()>,
) -> Result<()> {
    super::job_lease::until_settled(
        lease,
        || {
            transfer(worker)?;
            drain(worker)
        },
        || pending(worker),
    )
}
/// Transfer before choosing a queued review, including when another is active.
/// The separate lock prevents concurrent wakes from transferring a batch twice.
pub fn transfer(worker: &str) -> Result<()> {
    let directory = Store::directory(worker)?.join("checkpoint-events");
    if !directory.exists() {
        return Ok(());
    }
    let _transfer = Store::delivery_at(&directory, true)?;
    let mut paths = fs::read_dir(&directory)?
        .map(|e| e.map(|e| e.path()))
        .collect::<std::io::Result<Vec<_>>>()?;
    paths.sort();
    paths.retain(|p| p.extension().is_some_and(|e| e == "json"));
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
    // Ownership transfers once durable in the queue, even while another review
    // holds its lock. Waiting for native submission re-enqueued this same batch
    // on every follow-up and kept an idle observer reviewing its own triggers.
    for trigger in batches(&events) {
        super::queue::enqueue(worker, Purpose::Checkpoint, &turn, trigger)?;
    }
    for path in paths {
        fs::remove_file(path)?;
    }
    Ok(())
}
pub fn batches(events: &[Value]) -> Vec<Value> {
    let mut light: Vec<(String, Vec<Value>)> = vec![];
    let mut heavy = vec![];
    for event in events {
        if event["review_class"] == "commit"
            && matches!(
                event["kind"].as_str(),
                Some("native_head" | "native_phase_claim")
            )
        {
            let head = event["head"].as_str().unwrap_or("");
            if let Some((_, items)) = light.iter_mut().find(|(h, _)| h == head) {
                items.push(event.clone());
            } else {
                light.push((head.into(), vec![event.clone()]));
            }
        } else {
            heavy.push(event.clone());
        }
    }
    let mut batches = vec![];
    if !heavy.is_empty() {
        batches.push(json!({"review_class":"gate","events":heavy}));
    }
    batches.extend(light.into_iter().map(|(head,events)|json!({"review_class":"commit","head":if head.is_empty(){None}else{Some(head)},"events":events})));
    batches
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
