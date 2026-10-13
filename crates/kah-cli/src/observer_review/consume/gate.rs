use super::*;
/// The caller owns the main store lock until the durable handoff is saved.
pub(super) fn handoff_locked(
    store: &Store,
    state: &mut State,
    request: &super::super::state::Request,
    report: &Report,
    actions: &[Value],
    submission: &super::super::report_revision::Submission,
) -> Result<bool> {
    if request.purpose != Purpose::Checkpoint || super::super::commit_packet::is_commit(request) {
        return Ok(false);
    }
    let home = PathBuf::from(env::var_os("HOME").ok_or("REVIEW_HOME stage=handoff")?);
    let end = tokens(&home, store, state);
    super::super::gate_host::handoff_locked(store, state, request, report, actions, submission, end)
}
pub(crate) fn gate_decision(
    home: &Path,
    request: &super::super::state::Request,
    actions: &[Value],
    report: &Report,
) -> Result<(judge::Judgment, Value, Value)> {
    timed::decision(home, request, actions, report)
}
pub(crate) fn finish_gate_host(
    store: &Store,
    state: &mut State,
    job: &super::super::gate_host::Job,
    result: &Value,
    input: &Value,
    judgment: &judge::Judgment,
) -> Result<()> {
    let version = question_version(&job.request, &job.report);
    persist_evidence(
        store,
        &job.request,
        &job.report,
        result,
        input,
        judgment,
        version,
    )?;
    timed::record_effects(store, state, &job.request, &job.report, result, judgment)?;
    commit_result(
        store,
        state,
        &job.request,
        result.clone(),
        job.native_tokens_end,
    )?;
    Ok(())
}
