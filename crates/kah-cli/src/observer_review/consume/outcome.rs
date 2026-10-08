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
    let reason=match result {
        Err(e)=>e.to_string(),
        Ok(()) if before.is_some() && before==after=>"REVIEW_CONSUME_HANDOFF stage=consume: pending request retained for host/preparation continuation".into(),
        Ok(())=>"REVIEW_CONSUME_COMPLETED stage=consume".into(),
    };
    let decision = after
        .as_deref()
        .map(|id| super::super::preparation::active_decision(worker, id))
        .transpose()?;
    record(
        worker,
        json!({"kind":"return","success":result.is_ok(),"reason":reason,"before":before,"after":after,"active_decision":decision,"elapsed_ms":elapsed_ms}),
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
    finished(worker, before, started.elapsed().as_millis(), &result)?;
    result
}

pub(crate) fn stage(worker: &str, request: &str, stage: &str) -> Result<()> {
    record(
        worker,
        json!({"kind":"stage","request":request,"stage":stage}),
    )
}
