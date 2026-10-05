//! Undeliverable reviews are named on the durable host reviewer channel.
use super::*;
use crate::observer_review::state::State;

pub fn notify(state: &State) -> Result<()> {
    let root = Store::directory(&state.worker.session)?;
    let Some(notice) = super::super::state::read::<Value>(&root.join("redelivery-notice.json"))?
    else {
        return Ok(());
    };
    let prior: Option<Value> =
        super::super::state::read(&root.join("redelivery-reviewer-notice.json"))?;
    if prior
        .as_ref()
        .is_some_and(|p| p["notice"] == notice && p["submitted"] == true)
    {
        return Ok(());
    }
    let text = format!(
        "[kah-builder] [kah-observer-failure] worker={} request={} {}",
        state.worker.session,
        notice["request"],
        notice["text"].as_str().unwrap_or("Review remains queued.")
    );
    let delivered = state
        .worker
        .socket
        .as_deref()
        .ok_or_else(|| "REVIEW_HERDR_REQUIRED stage=notice".into())
        .and_then(|socket| {
            crate::run_pane::invoke_at(&["agent", "prompt", "kah-reviewer", &text], Some(socket))
        });
    crate::unattended_report::save(
        &root,
        "redelivery-reviewer-notice.json",
        &json!({"notice":notice,"submitted":delivered.is_ok(),"error":delivered.err().map(|e|e.to_string())}),
    )?;
    Ok(())
}
