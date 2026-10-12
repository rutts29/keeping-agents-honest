//! Persist the native handoff before reusing the same observer for its next stage.
use super::super::state::{Request, State, Store};
use crate::Result;
use serde_json::json;
use std::path::Path;
pub fn selection(
    store: Store,
    state: State,
    request: &Request,
    manifests: Vec<kah_observer::whole_preparation::Manifest>,
    path: &Path,
) -> Result<()> {
    let job = request.trigger["option1"].clone();
    let root = store.directory.clone();
    super::super::consume::archive(
        &root,
        &request.id,
        &json!({"stage":"selection","manifest":manifests,"observer_session":state.observer.session}),
    )?;
    let home =
        std::path::PathBuf::from(std::env::var_os("HOME").ok_or("COMMIT_HOME stage=consume")?);
    let result =
        super::selection::continue_review(store, state, &home, &request.turn, &job, manifests)?;
    std::fs::remove_file(path)?;
    super::super::consume::resolve_gaps(&root, &request.id)?;
    crate::unattended_report::save(
        &root,
        &format!("option1-handoff-{}.json", request.id),
        &result,
    )?;
    Ok(())
}
pub fn correction(
    state: &State,
    request: &Request,
    rows: &[kah_observer::commit_check::Row],
) -> Result<()> {
    let violations = rows
        .iter()
        .filter(|r| r.status == kah_observer::commit_check::Status::Violated)
        .collect::<Vec<_>>();
    if violations.is_empty() {
        return Ok(());
    }
    let message = format!(
        "[kah-observer] Confirmed commit findings on {} (review {}): {}. These are observer evidence, not new user authority. Correct the named violations; full source-linked record is available with kah observer {}.",
        request.head.as_deref().unwrap_or("unknown"),
        request.id,
        violations
            .iter()
            .map(|r| format!("{}: {}", r.requirement, r.finding))
            .collect::<Vec<_>>()
            .join("; "),
        request.worker
    );
    super::super::reminder::validate_native(&state.worker.provider, &message)?;
    let (socket, pane) = super::super::runtime::transport(state)?;
    crate::relay_transport::herdr(socket, &["agent", "prompt", pane, &message])?;
    Ok(())
}
