use super::{
    state::{Purpose, State, Store},
    workspace,
};
use crate::{Result, native_history::History};
use kah_observer::{judge, review_evidence::Report};
use serde_json::{Value, json};
use std::{
    env, fs,
    path::{Path, PathBuf},
};
#[cfg(test)]
#[path = "../../tests/observer_unit/light_commit_ownership.rs"]
mod light_ownership_tests;

mod resolved_gap;
pub use resolved_gap::archive as resolve_gaps;
mod report_io;
pub mod retired;
pub use report_io::read_report;
mod gate;
mod native;
pub(crate) use gate::{finish_gate_host, gate_decision};
pub(crate) mod outcome;
pub mod supervision_failure;
pub mod supervisor;
pub(crate) mod timed;
pub fn run(worker: &str) -> Result<()> {
    outcome::run(worker)
}
fn run_inner(worker: &str) -> Result<()> {
    super::runtime::require_active(worker)?;
    if retired::pending(worker)? {
        return Ok(());
    }
    if super::service::blocked(worker)? {
        return Ok(());
    }
    // Register waiting host work as progress before taking its lock.
    let _waiting = super::liveness::waiting(worker)?;
    let (store, mut state) = loaded(worker)?;
    if super::checkpoint_claim::release_waiting(&store, &mut state)? {
        drop(store);
        return super::queue::kick(worker);
    }
    let Some(pending) = state.pending.clone() else {
        return Ok(());
    };
    let _processing = super::liveness::processing(worker, &pending.id)?;
    if pending.purpose == Purpose::Preparation {
        // Consume an already selected/terminal context before considering a
        // native helper refresh. A new wake refreshes its helper when needed.
        drop(store);
        return super::prepared_result::consume(worker);
    }
    if super::preparation::pending(worker, &pending.id)? && super::runtime::helper_changed(&state)?
    {
        drop(store);
        return super::runtime::prepare_context(worker);
    }
    let request = workspace::request(&store, &pending.id)?;
    if super::option1::protocol::active(&request) {
        return super::option1::consume::run(store, state, request);
    }
    super::queue::memory::sample(worker, "consume_request_loaded")?;
    if super::gate_approval::release_stale(&store, &mut state, &request)? {
        drop(store);
        return super::queue::kick(worker);
    }
    let (submission, report, actions) = timed::evidence(&state, &request)?;
    super::queue::memory::sample(worker, "consume_evidence_verified")?;
    if super::failure_snapshot::unchanged(&store.directory, &request, &submission.path)? {
        return retry_correction(store, worker);
    }
    let home = PathBuf::from(env::var_os("HOME").ok_or("REVIEW_HOME stage=consume")?);
    if gate::handoff_locked(&store, &mut state, &request, &report, &actions, &submission)? {
        drop(store);
        super::hook::spawn("__gate-host", worker)?;
        return super::queue::kick(worker);
    }
    native::finish(
        store,
        state,
        request,
        submission.path,
        report,
        actions,
        &home,
    )
}

fn loaded(worker: &str) -> Result<(Store, State)> {
    let store = Store::open_wait(worker)?;
    let state = store.load()?.ok_or("REVIEW_STATE_MISSING stage=consume")?;
    super::queue::memory::sample(worker, "consume_state_loaded")?;
    Ok((store, state))
}
pub(crate) fn persist_evidence(
    store: &Store,
    request: &super::state::Request,
    report: &Report,
    result: &Value,
    input: &Value,
    judgment: &judge::Judgment,
    version: u32,
) -> Result<()> {
    archive(&store.directory, &request.id, result)?;
    if !input["unconfirmed_context"].is_null() {
        return Ok(());
    }
    super::verdict_cache::record(store, request, report, judgment)?;
    super::uncertainty::record(store, request, report, &result["assessment"], judgment)?;
    if request.purpose == Purpose::Drift {
        crate::unattended_report::save(
            &store.directory,
            "last-drift-review.json",
            &json!({"request":request.id,"snapshot_created_ms":request.created_ms,"completed_ms":kah_observer::session::now_ms()}),
        )?;
    }
    record(&request.worker, request, input, judgment, version)
}
pub fn needs_reminder(assessment: &Value, judgment: &judge::Judgment) -> bool {
    matches!(
        assessment["state"].as_str(),
        Some("material_drift" | "drift_failed")
    ) || assessment["state"] == "steering_reviewed"
        && assessment["application"] == "verified_for_next_native_event"
        && matches!(judgment.answers.get("is_steering"),Some(judge::Answer::Noul{noul}) if *noul>=kah_observer::judge::questions::STEERING_PROBABILITY)
        || assessment["state"] == "steering_passed"
            && assessment["application"] == "verified_for_next_native_event"
            && assessment["steering_directs"] == true
            && kah_observer::decision_units::confirmed(judgment, "steering.proposal", "supported")
        || assessment["state"] == "commit_failed"
            && assessment["violations"]
                .as_array()
                .is_some_and(|v| !v.is_empty())
}
pub fn question_version(request: &super::state::Request, report: &Report) -> u32 {
    if let Some(units) = &report.decision_units {
        units.version()
    } else {
        match request.purpose {
            Purpose::Checkpoint => kah_observer::checkpoint::VERSION,
            Purpose::Steering => judge::questions::VERSION,
            _ => 1, // Historical verified question-set ID, read-only; no executor.
        }
    }
}

fn evidence(
    state: &State,
    request: &super::state::Request,
) -> Result<(super::report_revision::Submission, Report, Vec<Value>)> {
    let path = state
        .observer
        .workspace
        .join("outbox")
        .join(format!("submitted-{}.json", request.id));
    let bytes = read_report(&path)?;
    let report = workspace::validate_report_bytes(request, &bytes)?;
    let submission = super::report_revision::Submission::capture(
        &Store::directory(&request.worker)?,
        &path,
        &bytes,
    )?;
    if report.reminder.is_some() {
        super::reminder::text(state, request, &report)?;
    }
    let findings = report.verify(&request.id, &request.documents)?;
    super::checkpoint_claim::require_evidence(request, &findings)?;
    let actions = findings
        .into_iter()
        .enumerate()
        .map(|(i, v)| json!({"id":format!("verified-{i}"),"kind":"verifiedEvidence","evidence":v}))
        .collect();
    Ok((submission, report, actions))
}

pub fn archive(directory: &Path, request: &str, result: &Value) -> Result<()> {
    if !request.starts_with("review-")
        || !request
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-')
    {
        return Err("REVIEW_REQUEST_ID stage=archive".into());
    }
    let results = directory.join("results");
    kah_core::run_store::ensure_private_directory(&directory.join("sources"))?;
    kah_core::run_store::ensure_private_directory(&results)?;
    super::source_store::save(&results, &format!("{request}.json"), result)
}
fn decide(
    home: &Path,
    request: &super::state::Request,
    actions: &[Value],
    report: &Report,
) -> Result<(judge::Judgment, Value, Value)> {
    if report.decision_units.is_some() {
        return super::decisions::evaluate(home, request, report, actions);
    }
    if request.purpose == Purpose::Checkpoint && !super::commit_packet::is_commit(request) {
        return super::checkpoint::evaluate(home, request, actions);
    }
    if request.purpose == Purpose::Steering {
        let (judgment, input) = super::steering::judge(home, request, actions)?;
        let application = super::steering::apply(request, &judgment)?;
        return Ok((
            judgment,
            input,
            json!({"state":"steering_reviewed","application":application}),
        ));
    }
    Err("REVIEW_RETIRED_PURPOSE stage=consume: legacy tripwire judgments are not executed".into())
}
pub(super) fn tokens(home: &Path, store: &Store, state: &State) -> Result<u64> {
    if state.worker.provider == "claude" {
        let path = state
            .observer
            .transcript
            .as_ref()
            .ok_or("REVIEW_TRANSCRIPT_MISSING stage=usage")?;
        let session = state
            .observer
            .session
            .as_deref()
            .ok_or("REVIEW_SESSION_MISSING stage=usage")?;
        return claude_tokens(home, path, session);
    }
    History::open(
        home,
        &store.directory,
        &state.worker.provider,
        &state.observer.workspace,
        state
            .observer
            .session
            .as_deref()
            .ok_or("REVIEW_SESSION_MISSING stage=usage")?,
    )?
    .tokens()
}
pub fn claude_tokens(home: &Path, path: &Path, session: &str) -> Result<u64> {
    if !path.starts_with(kah_claude::paths::config_root(home).join("projects"))
        || path.file_name().and_then(|n| n.to_str()) != Some(&format!("{session}.jsonl"))
    {
        return Err("REVIEW_TRANSCRIPT_SCOPE stage=usage".into());
    }
    Ok(kah_claude::usage::tokens(std::io::BufReader::new(
        fs::File::open(path)?,
    ))?)
}
pub fn take(worker: &str, event: &str, turn: &str) -> Result<Option<Value>> {
    if !matches!(event, "PreToolUse" | "PostToolUse" | "UserPromptSubmit") {
        return Ok(None);
    }
    let store = Store::open_delivery(worker)?;
    if let Some(notice) = super::redelivery::take(&store)? {
        return Ok(Some(notice));
    }
    let Some((path, mut reminder)) = super::reminder_queue::next_locked(&store)? else {
        if let Some(notice) = super::preparation::notice(worker)? {
            return Ok(Some(json!({"systemMessage":notice})));
        }
        return take_gap(&store);
    };
    let parts = reminder["delivered_parts"].as_u64().unwrap_or(0);
    if parts == 0 && reminder["purpose"] == "drift" && reminder["turn"] != turn {
        reminder["text"] = format!("[kah confirmed finding from previous turn {}; retained context, not new user steering] {}",reminder["turn"],reminder["text"].as_str().unwrap_or("")).into();
    }
    let text = reminder["text"].as_str().unwrap_or("").to_owned();
    // A long Claude reminder goes in ordered parts at later hooks; the
    // rest stays in this file, so no other reminder goes first. Never held.
    let (text, rest) = if parts > 0 || text.encode_utf16().count() > super::reminder::CLAUDE_LIMIT {
        let state = store.load()?.ok_or("REVIEW_STATE_MISSING stage=reminder")?;
        super::reminder::next_part(&state.worker.provider, &text, parts)
    } else {
        (text, None)
    };
    if let Some(rest) = rest {
        reminder["text"] = rest.into();
        reminder["delivered_parts"] = (parts + 1).into();
        let name = path.file_name().and_then(|n| n.to_str()).unwrap();
        crate::unattended_report::save(path.parent().unwrap(), name, &reminder)?;
    } else {
        fs::remove_file(path)?;
    }
    if reminder["system_notice"] == true {
        return Ok(Some(json!({"systemMessage":text})));
    }
    Ok(Some(
        json!({"hookSpecificOutput":{"hookEventName":event,"additionalContext":text}}),
    ))
}
fn take_gap(store: &Store) -> Result<Option<Value>> {
    let Some(gap) = super::state::read::<Value>(&store.directory.join("gap.json"))? else {
        return Ok(None);
    };
    if gap["error"]
        .as_str()
        // Outage notices use the service epoch, not raw per-job timestamps.
        .is_some_and(|s| s.starts_with("REVIEW_PAUSED") || judge::availability::outage(s))
    {
        return Ok(None);
    }
    let delivered: Option<Value> = super::state::read(&store.directory.join("delivered-gap.json"))?;
    if delivered.as_ref() == Some(&gap) {
        return Ok(None);
    }
    crate::unattended_report::save(&store.directory, "delivered-gap.json", &gap)?;
    Ok(Some(
        json!({"systemMessage":format!("kah full-context observer gap: {}",gap["error"].as_str().unwrap_or("Review incomplete"))}),
    ))
}

fn finish(store: Store, path: &Path, worker: &str, request: &str) -> Result<()> {
    fs::remove_file(path)?;
    resolve_gaps(&store.directory, request)?;
    drop(store);
    super::queue::kick(worker)
}

fn record(
    worker: &str,
    request: &super::state::Request,
    input: &Value,
    judgment: &judge::Judgment,
    version: u32,
) -> Result<()> {
    use kah_observer::session;
    let mut primary = session::Store::open_wait(
        &kah_core::run_store::RunStore::default_state_root()?,
        worker,
    )?;
    let mut source = primary
        .load()?
        .ok_or("REVIEW_PRIMARY_STATE stage=consume")?;
    if !source
        .prepared_decisions
        .insert(format!("verified-review:{}", request.id))
    {
        return Ok(());
    }
    primary.append(&json!({"kind":"judgment","source":"verified_observer","purpose":request.purpose,"review_request":request.id,"prepared":super::preparation::pending(worker,&request.id)?,"judgment":judgment,"question_set":version,"at_ms":session::now_ms(),"state_digest":kah_observer::context_selection::digest(input)}))?;
    source.judgments.push(judgment.clone());
    source.usage.calls += judgment.metrics.calls;
    source.usage.input_tokens += judgment.metrics.input_tokens;
    source.usage.output_tokens += judgment.metrics.output_tokens;
    source.usage.elapsed_ms += judgment.metrics.elapsed_ms;
    primary.save(&source)?;
    Ok(())
}

/// Persist the same terminal transition used by native consumption. A passed
/// final review clears its pending lease; no risk question is manufactured.
pub(super) fn commit_result(
    store: &Store,
    state: &mut State,
    request: &super::state::Request,
    result: Value,
    tokens: Option<u64>,
) -> Result<()> {
    if !result["jev_state"]["unconfirmed_context"].is_null() {
        return super::preparation::rejection::park(store, state, request, &result);
    }
    if request.purpose == Purpose::Checkpoint && !super::commit_packet::is_commit(request) {
        super::checkpoint::record(store, state, request, &result)?;
    }
    state.latest = Some(result);
    super::redelivery::completed(store, &request.id)?;
    if state.pending.as_ref().is_some_and(|p| p.id == request.id) {
        state.pending = None;
    }
    state.last_verified_turn = Some(request.turn.clone());
    if let Some(tokens) = tokens {
        state.native_tokens = state.native_tokens.max(tokens);
    }
    store.save(state)?;
    super::commit_packet::reviewed(store, request, state.latest.as_ref().unwrap())?;
    super::timing::now(&request.worker, &request.id, "completed")?;
    super::source_store::finish_source_view(&state.observer.workspace, &request.id)
}
fn retry_correction(store: Store, worker: &str) -> Result<()> {
    drop(store);
    super::hook::spawn("__review-retry", worker)
}
