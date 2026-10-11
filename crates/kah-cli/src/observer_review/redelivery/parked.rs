//! A parked validation is an observation, not ownership of the review slot.
use super::*;
pub fn release(store: &Store, request: &str, gap: &Value) -> Result<()> {
    let Some(mut state) = store.load()? else {
        return Ok(());
    };
    let Some(pending) = state.pending.as_ref().filter(|p| p.id == request).cloned() else {
        return Ok(());
    };
    let directory = store.directory.join("parked-validation");
    kah_core::run_store::ensure_private_directory(&directory)?;
    super::super::source_store::save(
        &directory,
        &format!("{request}.json"),
        &json!({"pending":pending,"gap":gap,"request_path":store.directory.join("requests").join(format!("{request}.json")),"at_ms":kah_observer::session::now_ms(),"status":"validation_blocked_unchanged"}),
    )?;
    let captured: Option<super::super::state::Request> = super::super::state::read(
        &store
            .directory
            .join("requests")
            .join(format!("{request}.json")),
    )?;
    if pending.purpose == super::super::state::Purpose::Checkpoint
        && !captured
            .as_ref()
            .is_some_and(super::super::commit_packet::is_commit)
    {
        state.checkpoint = Some(
            json!({"head":pending.head,"request":request,"passed":null,"state":"checkpoint_validation_blocked","unproven":["report_validation"],"at_ms":kah_observer::session::now_ms()}),
        );
    }
    state.pending = None;
    store.save(&state)?;
    super::super::source_store::finish_source_view(&state.observer.workspace, request)?;
    let path = store.directory.join("gap.json");
    if super::super::state::read::<Value>(&path)?.as_ref() == Some(gap) {
        std::fs::remove_file(path)?;
    }
    Ok(())
}
