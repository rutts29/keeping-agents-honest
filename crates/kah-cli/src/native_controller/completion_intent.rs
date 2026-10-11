use crate::Result;
use kah_core::projection::ActiveProjection;
use kah_observer::{
    engine::Outcome,
    session::{self, Binding, Store},
};
use std::path::Path;

pub(super) enum Status {
    Ready(Box<ActiveProjection>),
    Pending(String),
}
pub(super) fn paused(base: &Path, binding: &Binding) -> Option<Status> {
    session::paused(base, &binding.session).then(|| Status::Pending("observer paused".into()))
}
pub(super) fn resolve(base: &Path, binding: &Binding, outcome: Outcome) -> Result<Status> {
    if let Some(status) = paused(base, binding) {
        return Ok(status);
    }
    if let Some(gap) = outcome.gap {
        return Ok(Status::Pending(format!("observer intake gap: {gap}")));
    }
    if let Some(projection) = outcome
        .projection
        .filter(|p| !p.goal.text.trim().is_empty())
    {
        return Ok(Status::Ready(Box::new(projection)));
    }
    // A no-op intake has no new projection. Recover authority from the latest
    // session/run binding, never from the controller task or its rendered report.
    let saved =
        Store::read(base, &binding.session)?.ok_or("RUN_INTENT_MISSING stage=completion")?;
    if saved.binding.project != binding.project || saved.binding.session != binding.session {
        return Err("RUN_OBSERVER_SCOPE stage=completion: persisted intent binding changed".into());
    }
    if let Some(gap) = saved.gap {
        return Ok(Status::Pending(format!(
            "observer intake gap: {}",
            gap.detail
        )));
    }
    let id = saved.run_id.ok_or("RUN_INTENT_MISSING stage=completion")?;
    let store = kah_core::run_store::RunStore::open_wait(base, &binding.project, &id)?;
    let projection = store.recover()?.projection;
    if projection.goal.text.trim().is_empty() {
        return Err("RUN_INTENT_MISSING stage=completion".into());
    }
    Ok(Status::Ready(Box::new(projection)))
}
pub(super) fn waiting(status: &Status) -> Option<&str> {
    match status {
        Status::Ready(_) => None,
        Status::Pending(reason) => Some(reason),
    }
}

#[cfg(test)]
#[path = "../../tests/native_controller_unit/completion_intent.rs"]
mod tests;
