//! All domains synchronize intent; only Mine/Research queue native rule rebuilds.
use kah_core::observation::ObservationSnapshot;
use kah_observer::{engine, session::Binding};
pub fn observe(binding: &Binding, turn: &str) -> Result<engine::Outcome, String> {
    after_sync(binding, engine::synchronize(binding, turn)?)
}
pub fn snapshot(
    binding: &Binding,
    turn: &str,
    snapshot: ObservationSnapshot,
) -> Result<engine::Outcome, String> {
    after_sync(
        binding,
        engine::synchronize_snapshot(binding, turn, snapshot)?,
    )
}
pub fn after_sync(binding: &Binding, outcome: engine::Outcome) -> Result<engine::Outcome, String> {
    // A detached UserPromptSubmit intake has no guaranteed later Stop dispatch.
    // Publish its distinct intent wake at successful sync, with no inference.
    if outcome.gap.is_some()
        || crate::observer_review::untrusted::host::scope(binding).map_err(|e| e.to_string())?
    {
        return Ok(outcome);
    }
    let staged = kah_observer::current_rules::stage(binding)?;
    let config =
        crate::observer_review::control::config(&binding.session).map_err(|e| e.to_string())?;
    let base = kah_core::run_store::RunStore::default_state_root().map_err(|e| e.to_string())?;
    if config.is_some_and(|c| c.enabled)
        && !kah_observer::session::paused(&base, &binding.session)
        && let Some(kah_core::run_store::RuleSync::Rebuild(r)) = staged
    {
        let trigger =
            crate::observer_review::option1::current_set::trigger(&r).map_err(|e| e.to_string())?;
        crate::observer_review::queue::enqueue(
            &binding.session,
            crate::observer_review::state::Purpose::Steering,
            "intent-sync",
            trigger,
        )
        .map_err(|e| e.to_string())?;
        crate::observer_review::queue::schedule(&binding.session).map_err(|e| e.to_string())?;
    }
    Ok(outcome)
}
