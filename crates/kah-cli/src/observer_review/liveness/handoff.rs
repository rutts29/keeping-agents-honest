//! An ACK transfers the request to host consumption, not another native turn.
use super::super::{
    preparation,
    state::{self, State, Store},
};
use crate::Result;
use serde_json::Value;

pub(crate) fn awaiting_host(state: &State) -> Result<bool> {
    let Some(pending) = &state.pending else {
        return Ok(false);
    };
    let root = Store::directory(&state.worker.session)?;
    let decision = preparation::active_decision(&state.worker.session, &pending.id)?;
    if preparation::pending(&state.worker.session, &decision)? {
        // The foreground selector validates before ACK. Recheck/copy that
        // exact manifest on the host; no Jev call and no native re-prompt.
        return preparation::accept(&state.worker.session, &decision);
    }
    let ready: Option<Value> = state::read(&root.join("report-ready.json"))?;
    if !ready.is_some_and(|r| r["request"] == pending.id)
        || !state
            .observer
            .workspace
            .join("outbox")
            .join(format!("submitted-{}.json", pending.id))
            .is_file()
    {
        return Ok(false);
    }
    let gap: Option<Value> = state::read(&root.join("gap.json"))?;
    Ok(!gap
        .and_then(|g| g["error"].as_str().map(str::to_owned))
        .is_some_and(|e| correction_required(&e)))
}
fn correction_required(error: &str) -> bool {
    // Delivery noise and a late missing-report event cannot undo an ACK.
    !super::super::delivery::wait_timeout(error)
        && ![
            "REVIEW_OBSERVER_STALLED",
            "REVIEW_SUBMISSION_UNCONFIRMED",
            "REVIEW_NOT_IDLE",
            "REVIEW_REPORT_MISSING",
        ]
        .iter()
        .any(|prefix| error.starts_with(prefix))
}

#[cfg(test)]
#[path = "../../../tests/observer_unit/ack_handoff.rs"]
mod tests;
