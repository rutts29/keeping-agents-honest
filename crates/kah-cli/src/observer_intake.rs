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
        stage_rules(binding, outcome)
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
        stage_rules(binding, outcome)
    }
}

fn stage_rules(binding: &Binding, outcome: engine::Outcome) -> Result<engine::Outcome, String> {
    // Durable staging only. The controller resumes the SAME native observer;
    // it must use direct review until current_rules::current returns a set.
    kah_observer::current_rules::stage(binding)?;
    Ok(outcome)
}
