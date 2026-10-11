//! Mine/Research synchronize intent; Untrusted keeps its legacy action checks.
use kah_core::observation::ObservationSnapshot;
use kah_observer::{engine, session::Binding};

pub fn legacy_checks(binding: &Binding) -> Result<bool, String> {
    crate::observer_review::control::enabled(binding)
        .map(|enabled| !enabled)
        .map_err(|e| e.to_string())
}

pub fn observe(
    binding: &Binding,
    turn: &str,
    allow_reminder: bool,
) -> Result<engine::Outcome, String> {
    if legacy_checks(binding)? {
        engine::observe(binding, turn, allow_reminder)
    } else {
        let outcome = engine::synchronize(binding, turn)?;
        after_sync(binding, outcome)
    }
}

pub fn snapshot(
    binding: &Binding,
    turn: &str,
    allow_reminder: bool,
    snapshot: ObservationSnapshot,
) -> Result<engine::Outcome, String> {
    if legacy_checks(binding)? {
        engine::observe_snapshot(binding, turn, allow_reminder, snapshot)
    } else {
        let outcome = engine::synchronize_snapshot(binding, turn, snapshot)?;
        after_sync(binding, outcome)
    }
}

pub fn after_sync(binding: &Binding, outcome: engine::Outcome) -> Result<engine::Outcome, String> {
    // A detached UserPromptSubmit intake has no guaranteed later Stop dispatch.
    // Publish its distinct intent wake at successful sync, with no inference.
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
