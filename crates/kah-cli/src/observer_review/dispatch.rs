use super::{
    control, hook,
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
    if let Some(notice) = super::artifacts::start_status(binding, input)? {
        return Ok(Some(notice));
    }
    if matches!(
        input["hook_event_name"].as_str(),
        Some("PostToolUse" | "Stop" | "UserPromptSubmit")
    ) {
        super::uncertainty::schedule(&binding.session)?;
    }
    if matches!(
        input["hook_event_name"].as_str(),
        Some("SessionStart" | "PostToolUse" | "Stop")
    ) {
        super::tags::schedule(&binding.session)?;
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
    super::queue::kick(&binding.session)?;
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
    let triggers = triggers(
        outcome.verification.take(),
        super::steering::pending(&binding.session)?,
    );
    if triggers.is_empty() {
        return Ok(());
    }
    for (purpose, trigger) in triggers {
        super::queue::enqueue(&binding.session, purpose, &config.turn, trigger)?;
    }
    hook::spawn("__review-wake", &binding.session)
}
pub fn triggers(verification: Option<Value>, steering: Option<Value>) -> Vec<(Purpose, Value)> {
    let mut triggers = vec![];
    if let Some(steering) = steering {
        triggers.push((Purpose::Steering, steering));
    }
    if let Some(verification) = verification {
        let purpose = if verification.get("preparation").is_some() {
            Purpose::Preparation
        } else {
            Purpose::Drift
        };
        triggers.push((purpose, verification));
    }
    triggers
}
pub fn run(worker: &str) -> Result<()> {
    super::queue::drain(worker)?;
    Ok(())
}
