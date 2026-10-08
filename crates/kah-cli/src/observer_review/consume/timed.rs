use super::*;
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
