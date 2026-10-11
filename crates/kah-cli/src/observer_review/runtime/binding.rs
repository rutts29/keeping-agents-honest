//! Native inventory distinguishes a missing pane from a replaced owner.
//! herdrdev/herdr v0.9.3 skills/herdr/SKILL.md: pane get/list, background split.
use super::*;

pub(super) fn ensure(state: &mut State) -> Result<bool> {
    if state.observer.pane.is_some() {
        let (socket, worker) = transport(state)?;
        let view = herdr(socket, &["pane", "get", worker])?;
        let workspace = view["result"]["pane"]["workspace_id"]
            .as_str()
            .ok_or("REVIEW_WORKSPACE_ID stage=observer_binding")?;
        let inventory = herdr(socket, &["pane", "list", "--workspace", workspace])?;
        if present(&inventory, state.observer.pane.as_deref().unwrap())? {
            control::owned_pane(state)?;
            return Ok(true);
        }
    }
    // Session/transcript and pending request survive pane loss. Native resume
    // starts the SAME observer with configured floor flags, without stealing focus.
    state.worker = crate::relay_store::endpoint(&state.worker.session)?;
    state.observer.pane = Some(create(state)?);
    state.observer.pid = None;
    state.observer.process_stamp = None;
    Ok(false)
}
pub fn present(inventory: &Value, pane: &str) -> Result<bool> {
    let panes = inventory["result"]["panes"]
        .as_array()
        .ok_or("REVIEW_PANE_INVENTORY stage=observer_binding")?;
    if panes.iter().any(|p| !p["pane_id"].is_string()) {
        return Err("REVIEW_PANE_INVENTORY stage=observer_binding".into());
    }
    Ok(panes.iter().any(|p| p["pane_id"] == pane))
}
