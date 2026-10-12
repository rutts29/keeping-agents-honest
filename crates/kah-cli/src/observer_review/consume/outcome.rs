//! Durable host-job outcomes include successful handoffs, not just errors.
use crate::{
    Result,
    observer_review::state::{self, Store},
};
use serde_json::{Value, json};
use std::{fs, io::Write, os::unix::fs::OpenOptionsExt};
pub(crate) fn record(worker: &str, event: Value) -> Result<()> {
    let root = Store::directory(worker)?;
    kah_core::run_store::ensure_private_directory(&root)?;
    let mut file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .mode(0o600)
        .open(root.join("consume-outcomes.jsonl"))?;
    writeln!(
        file,
        "{}",
        json!({"at_ms":kah_observer::session::now_ms(),"pid":std::process::id(),"event":event})
    )?;
    file.sync_all()?;
    Ok(())
}
pub fn pending(worker: &str) -> Result<Option<String>> {
    Ok(state::read_control(worker)?
        .and_then(|s| s.pending)
        .map(|p| p.id))
}
pub fn finished(
    worker: &str,
    before: Option<String>,
    elapsed_ms: u128,
    result: &Result<()>,
) -> Result<()> {
    let after = pending(worker)?;
    let gate_handoff = if let Some(request) = &before {
        super::super::gate_host::jobs(worker)?
            .iter()
            .any(|root| root.file_name().and_then(|name| name.to_str()) == Some(request.as_str()))
    } else {
        false
    };
    let reason=match result {
        Err(e)=>e.to_string(),
        Ok(()) if gate_handoff=>"REVIEW_CONSUME_HANDOFF stage=consume: durable gate host owns evaluation".into(),
        Ok(()) if before.is_some() && before==after=>"REVIEW_CONSUME_HANDOFF stage=consume: pending request retained for host/preparation continuation".into(),
        Ok(())=>"REVIEW_CONSUME_COMPLETED stage=consume".into(),
    };
    let decision = after
        .as_deref()
        .map(|id| super::super::preparation::active_decision(worker, id))
        .transpose()?;
    let waiting = result
        .as_ref()
        .err()
        .is_some_and(|e| super::super::failure::pending(e.as_ref()));
    record(
        worker,
        json!({"kind":"return","success":result.is_ok() || waiting,"operation_succeeded":result.is_ok(),"pending":waiting,"reason":reason,"before":before,"after":after,"active_decision":decision,"elapsed_ms":elapsed_ms}),
    )?;
    eprintln!("{reason}; elapsed_ms={elapsed_ms}");
    Ok(())
}

pub fn run(worker: &str) -> Result<()> {
    let started = std::time::Instant::now();
    let before = match pending(worker) {
        Ok(value) => value,
        Err(error) => {
            record(
                worker,
                json!({"kind":"return","success":false,"reason":error.to_string(),"stage":"before_start"}),
            )?;
            return Err(error);
        }
    };
    record(worker, json!({"kind":"started","pending":before}))?;
    super::super::queue::memory::sample(worker, "consume_start")?;
    let result = super::run_inner(worker);
    super::super::queue::memory::sample(worker, "consume_end")?;
    complete(
        worker,
        before,
        started.elapsed().as_millis(),
        result,
        || super::super::hook::spawn("__review-kick", worker),
    )
}

fn complete(
    worker: &str,
    before: Option<String>,
    elapsed_ms: u128,
    result: Result<()>,
    retry: impl FnOnce() -> Result<()>,
) -> Result<()> {
    finished(worker, before, elapsed_ms, &result)?;
    if result
        .as_ref()
        .err()
        .is_some_and(|e| super::super::failure::pending(e.as_ref()))
    {
        // run_inner has returned and released its review lock before requeueing.
        retry()?;
        return Ok(());
    }
    result
}

#[cfg(test)]
#[path = "../../../tests/observer_unit/busy_consume.rs"]
mod busy_tests;

pub(crate) fn stage(worker: &str, request: &str, stage: &str) -> Result<()> {
    record(
        worker,
        json!({"kind":"stage","request":request,"stage":stage}),
    )
}
