use super::{
    control, hook, runtime,
    state::{Purpose, Store},
};
use crate::Result;
use kah_observer::{engine::Outcome, session::Binding};
use serde_json::{Value, json};

pub fn register(binding: &Binding, input: &Value, provider: &str) -> Result<Option<Value>> {
    if kah_observer::session::paused(
        &kah_core::run_store::RunStore::default_state_root()?,
        &binding.session,
    ) {
        return Ok(Some(json!({})));
    }
    control::register(binding, input, provider)?;
    let config =
        control::config(&binding.session)?.ok_or("REVIEW_CONFIG_MISSING stage=dispatch")?;
    if !config.enabled {
        return Ok(None);
    }
    // Capture Stop before any pending tripwire/reminder can short-circuit the
    // hook. Checkpoints are independent of the tripwire's lease and verdict.
    if input["hook_event_name"] == "Stop" {
        super::claims::schedule(
            &binding.session,
            &config.turn,
            input["last_assistant_message"].as_str().unwrap_or(""),
        )?;
    }
    if super::events::pending(&binding.session)? {
        hook::spawn("__checkpoint-worker", &binding.session)?;
    }
    if let Some(state) = Store::read(&binding.session)?
        && let Some(pending) = state.pending
        && super::state::read::<Value>(
            &Store::directory(&binding.session)?.join("report-ready.json"),
        )?
        .is_some_and(|r| r["request"] == pending.id)
    {
        hook::spawn("__review-consume", &binding.session)?;
    }
    match super::consume::take(
        &binding.session,
        input["hook_event_name"].as_str().unwrap_or(""),
        &config.turn,
    ) {
        Err(e) if e.to_string().starts_with("REVIEW_BUSY") => Ok(None),
        other => other,
    }
}
pub fn outcome(binding: &Binding, outcome: &mut Outcome) -> Result<()> {
    if kah_observer::session::paused(
        &kah_core::run_store::RunStore::default_state_root()?,
        &binding.session,
    ) {
        return Ok(());
    }
    let Some(config) = control::config(&binding.session)? else {
        return Ok(());
    };
    if !config.enabled {
        return Ok(());
    }
    let (purpose, trigger) = if let Some(steering) = super::steering::pending(&binding.session)? {
        (Purpose::Steering, steering)
    } else if let Some(trigger) = outcome.verification.take() {
        (Purpose::Drift, trigger)
    } else {
        return Ok(());
    };
    let store = Store::open(&binding.session)?;
    if store.load()?.is_some_and(|s| {
        s.pending.is_some() || s.last_verified_turn.as_deref() == Some(&config.turn)
    }) {
        return Ok(());
    }
    crate::unattended_report::save(
        &store.directory,
        "wake.json",
        &json!({"purpose":purpose,"turn":config.turn,"trigger":trigger}),
    )?;
    drop(store);
    hook::spawn("__review-wake", &binding.session)
}
pub fn run(worker: &str) -> Result<()> {
    let path = Store::directory(worker)?.join("wake.json");
    let job: Value = super::state::read(&path)?.ok_or("REVIEW_WAKE_MISSING stage=dispatch")?;
    let purpose: Purpose = serde_json::from_value(job["purpose"].clone())?;
    runtime::wake(
        worker,
        purpose,
        job["turn"].as_str().ok_or("REVIEW_TURN stage=dispatch")?,
        job["trigger"].clone(),
    )?;
    Ok(())
}
