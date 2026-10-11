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
        engine::synchronize(binding, turn)
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
        engine::synchronize_snapshot(binding, turn, snapshot)
    }
}
