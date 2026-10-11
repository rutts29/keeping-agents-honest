//! All domains synchronize intent; only Mine/Research stage native rule rebuilds.
use kah_core::observation::ObservationSnapshot;
use kah_observer::{engine, session::Binding};

pub fn observe(binding: &Binding, turn: &str) -> Result<engine::Outcome, String> {
    stage_rules(binding, engine::synchronize(binding, turn)?)
}

pub fn snapshot(
    binding: &Binding,
    turn: &str,
    snapshot: ObservationSnapshot,
) -> Result<engine::Outcome, String> {
    stage_rules(
        binding,
        engine::synchronize_snapshot(binding, turn, snapshot)?,
    )
}

fn stage_rules(binding: &Binding, outcome: engine::Outcome) -> Result<engine::Outcome, String> {
    // Durable staging only. The controller resumes the SAME native observer;
    // it must use direct review until current_rules::current returns a set.
    if crate::observer_review::control::enabled(binding).map_err(|e| e.to_string())? {
        kah_observer::current_rules::stage(binding)?;
    }
    Ok(outcome)
}
