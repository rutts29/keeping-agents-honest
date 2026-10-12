use super::*;
pub(super) fn finish(
    store: Store,
    mut state: State,
    request: super::super::state::Request,
    path: PathBuf,
    report: Report,
    actions: Vec<Value>,
    home: &Path,
) -> Result<()> {
    let worker = state.worker.session.clone();
    let pending = state
        .pending
        .clone()
        .ok_or("REVIEW_PENDING_MISSING stage=consume")?;
    let decision = timed::decision(home, &request, &actions, &report);
    super::super::queue::memory::sample(&worker, "consume_decision_returned")?;
    if decision
        .as_ref()
        .is_err_and(|e| e.to_string().starts_with("JEV_PREPARATION_PENDING"))
    {
        drop(store);
        return super::super::runtime::prepare_context(&worker);
    }
    let (judgment, input, mut assessment) = decision?;
    super::super::gate_approval::guard_report(
        &store.directory,
        &request,
        &report,
        &mut assessment,
    )?;
    let version = question_version(&request, &report);
    let tokens = tokens(home, &store, &state);
    let result = json!({"request":request.id,"purpose":request.purpose,"head":request.head,"observer_session":state.observer.session,"pane":state.observer.pane,"report":report,"jev_state":input,"judgment":judgment,"verified_question_set":version,"assessment":assessment,"native_tokens":tokens.as_ref().ok().zip(pending.native_tokens_before).map(|(n,before)|n.saturating_sub(before)),"usage_gap":tokens.as_ref().err().map(ToString::to_string).or_else(||pending.native_tokens_before.is_none().then(||"REVIEW_USAGE_BASELINE_MISSING".into())),"elapsed_ms":kah_observer::session::now_ms().saturating_sub(pending.created_ms)});
    persist_evidence(
        &store, &request, &report, &result, &input, &judgment, version,
    )?;
    timed::record_effects(&store, &state, &request, &report, &result, &judgment)?;
    commit_result(&store, &mut state, &request, result, tokens.ok())?;
    if request.purpose == Purpose::Checkpoint && !super::super::commit_packet::is_commit(&request) {
        super::super::checkpoint::notify(&state, &request, state.latest.as_ref().unwrap())?;
    }
    super::finish(store, &path, &worker, &request.id)
}
