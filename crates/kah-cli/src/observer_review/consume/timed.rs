use super::*;
pub(super) fn record_effects(
    store: &Store,
    state: &State,
    request: &super::super::state::Request,
    report: &Report,
    result: &Value,
    judgment: &judge::Judgment,
) -> Result<()> {
    if needs_reminder(&result["assessment"], judgment) {
        super::super::reminder::save(store, request, report)?;
    }
    if result["jev_state"]["unconfirmed_context"].is_null() {
        super::super::component_map::producer::record(
            store,
            request,
            &state.observer.workspace,
            report,
        )?;
    }
    Ok(())
}
pub(super) fn evidence(
    state: &State,
    request: &super::super::state::Request,
) -> Result<(PathBuf, Report, Vec<Value>)> {
    super::super::timing::now(&request.worker, &request.id, "validation_started")?;
    let evidence = super::evidence(state, request)?;
    super::super::timing::now(&request.worker, &request.id, "validated")?;
    Ok(evidence)
}
pub(super) fn decision(
    home: &Path,
    request: &super::super::state::Request,
    actions: &[Value],
    report: &Report,
) -> Result<(judge::Judgment, Value, Value)> {
    super::super::timing::now(&request.worker, &request.id, "judgment_started")?;
    let result = super::decide(home, request, actions, report)?;
    super::super::timing::now(&request.worker, &request.id, "judged")?;
    Ok(result)
}
