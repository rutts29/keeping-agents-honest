use super::{
    state::{Purpose, State, Store},
    workspace,
};
use crate::{Result, native_history::History};
use kah_observer::{judge, review_evidence::Report};
use serde_json::{Value, json};
use std::{
    env,
    fs::{self, OpenOptions},
    io::Read,
    os::unix::fs::OpenOptionsExt,
    path::{Path, PathBuf},
};
#[cfg(test)]
#[path = "../../tests/observer_unit/light_commit_ownership.rs"]
mod light_ownership_tests;

/// Open first, then inspect the descriptor: no symlinks, devices or FIFO waits.
pub fn read_report(path: &Path) -> Result<Vec<u8>> {
    let mut file = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(path)?;
    let meta = file.metadata()?;
    if !meta.is_file() {
        return Err("REVIEW_REPORT_TYPE stage=consume".into());
    }
    let mut bytes = vec![];
    file.read_to_end(&mut bytes)?;
    Ok(bytes)
}
pub fn run(worker: &str) -> Result<()> {
    super::runtime::require_active(worker)?;
    // Register before lock acquisition: waiting behind legitimate host work
    // is a controller handoff, not an idle thinker needing another prompt.
    let observed = Store::read(worker)?;
    let _waiting = observed
        .as_ref()
        .and_then(|s| s.pending.as_ref())
        .map(|p| super::liveness::processing(worker, &p.id))
        .transpose()?;
    let store = Store::open_wait(worker)?;
    let mut state = store.load()?.ok_or("REVIEW_STATE_MISSING stage=consume")?;
    if super::checkpoint_claim::release_waiting(&store, &mut state)? {
        drop(store);
        return super::queue::kick(worker);
    }
    let Some(pending) = state.pending.clone() else {
        return Ok(());
    };
    let _processing = super::liveness::processing(worker, &pending.id)?;
    if super::preparation::pending(worker, &pending.id)? && super::runtime::helper_changed(&state)?
    {
        drop(store);
        return super::runtime::prepare_context(worker);
    }
    if pending.purpose == Purpose::Preparation {
        drop(store);
        return super::prepared_result::consume(worker);
    }
    let request = workspace::request(&store, &pending.id)?;
    if super::gate_approval::release_stale(&store, &mut state, &request)? {
        drop(store);
        return super::queue::kick(worker);
    }
    let (path, report, actions) = timed_evidence(&state, &request)?;
    if super::failure_snapshot::unchanged(&store.directory, &request, &path)? {
        return retry_correction(store, worker);
    }
    let home = PathBuf::from(env::var_os("HOME").ok_or("REVIEW_HOME stage=consume")?);
    let decision = timed_decision(&home, &request, &actions, &report);
    if decision
        .as_ref()
        .is_err_and(|e| e.to_string().starts_with("JEV_PREPARATION_PENDING"))
    {
        drop(store);
        return super::runtime::prepare_context(worker);
    }
    let (judgment, input, mut assessment) = decision?;
    super::gate_approval::guard_report(&store.directory, &request, &report, &mut assessment)?;
    let version = question_version(&request, &report);
    let tokens = tokens(&home, &store, &state);
    let result = json!({"request":request.id,"purpose":request.purpose,"head":request.head,"observer_session":state.observer.session,"pane":state.observer.pane,"report":report,"jev_state":input,"judgment":judgment,"verified_question_set":version,"assessment":assessment,"native_tokens":tokens.as_ref().ok().zip(pending.native_tokens_before).map(|(n,before)|n.saturating_sub(before)),"usage_gap":tokens.as_ref().err().map(ToString::to_string).or_else(||pending.native_tokens_before.is_none().then(||"REVIEW_USAGE_BASELINE_MISSING".into())),"elapsed_ms":kah_observer::session::now_ms().saturating_sub(pending.created_ms)});
    persist_evidence(
        &store, &request, &report, &result, &input, &judgment, version,
    )?;
    if needs_reminder(&assessment, &judgment) {
        super::reminder::save(&store, &request, &report)?;
    }
    commit_result(&store, &mut state, &request, result, tokens.ok())?;
    if request.purpose == Purpose::Checkpoint && !super::commit_packet::is_commit(&request) {
        super::checkpoint::notify(&state, &request, state.latest.as_ref().unwrap())?;
    }
    finish(store, &path, worker)
}
fn persist_evidence(
    store: &Store,
    request: &super::state::Request,
    report: &Report,
    result: &Value,
    input: &Value,
    judgment: &judge::Judgment,
    version: u32,
) -> Result<()> {
    archive(&store.directory, &request.id, result)?;
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
            _ => kah_observer::verified_judgment::VERSION,
        }
    }
}

fn evidence(
    state: &State,
    request: &super::state::Request,
) -> Result<(PathBuf, Report, Vec<Value>)> {
    let path = state
        .observer
        .workspace
        .join("outbox")
        .join(format!("submitted-{}.json", request.id));
    let report = workspace::validate_report_bytes(request, &read_report(&path)?)?;
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
    Ok((path, report, actions))
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
    if request.purpose != Purpose::Drift {
        return Err("REVIEW_PURPOSE_PENDING stage=consume".into());
    }
    let mut input = json!({"intent":request.intent,"event":{"actions":actions,"agent_text":"","user_text":""},"trigger":request.trigger});
    crate::privacy::scrub(&mut input);
    if super::preparation::pending(&request.worker, &request.id)? {
        super::preparation::accept(&request.worker, &request.id)?;
    }
    let (judgment, input) = super::preparation::evaluate(
        home,
        request,
        &input,
        &kah_observer::verified_judgment::questions(),
    )?;
    let failures = request.intent["failures"].as_array().map_or(0, Vec::len);
    let assessment = serde_json::to_value(kah_observer::verified_judgment::assess(
        &judgment, actions, failures,
    ))?;
    Ok((judgment, input, assessment))
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
    if !path.starts_with(home.join(".claude/projects"))
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
    if reminder["purpose"] == "drift" && reminder["turn"] != turn {
        reminder["text"] = format!("[kah confirmed finding from previous turn {}; retained context, not new user steering] {}",reminder["turn"],reminder["text"].as_str().unwrap_or("")).into();
    }
    let provider = store
        .load()?
        .ok_or("REVIEW_STATE_MISSING stage=reminder")?
        .worker
        .provider;
    if let Err(error) =
        super::reminder::validate_native(&provider, reminder["text"].as_str().unwrap_or(""))
    {
        drop(store);
        super::hook::gap(worker, &error)?;
        return Ok(Some(
            json!({"systemMessage":format!("kah observer reminder gap: {error}")}),
        ));
    }
    fs::remove_file(path)?;
    if reminder["system_notice"] == true {
        return Ok(Some(json!({"systemMessage":reminder["text"]})));
    }
    Ok(Some(
        json!({"hookSpecificOutput":{"hookEventName":event,"additionalContext":reminder["text"]}}),
    ))
}
fn take_gap(store: &Store) -> Result<Option<Value>> {
    let Some(gap) = super::state::read::<Value>(&store.directory.join("gap.json"))? else {
        return Ok(None);
    };
    if gap["error"]
        .as_str()
        .is_some_and(|s| s.starts_with("REVIEW_PAUSED"))
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

fn finish(store: Store, path: &Path, worker: &str) -> Result<()> {
    fs::remove_file(path)?;
    let gap = store.directory.join("gap.json");
    if gap.exists() {
        fs::remove_file(gap)?;
    }
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
    if request.purpose == Purpose::Checkpoint && !super::commit_packet::is_commit(request) {
        super::checkpoint::record(store, state, request, &result)?;
    }
    state.latest = Some(result);
    super::redelivery::completed(store, &request.id)?;
    state.pending = None;
    state.last_verified_turn = Some(request.turn.clone());
    if let Some(tokens) = tokens {
        state.native_tokens = tokens;
    }
    store.save(state)?;
    super::commit_packet::reviewed(store, request)?;
    super::timing::now(&request.worker, &request.id, "completed")
}
fn timed_evidence(
    state: &State,
    request: &super::state::Request,
) -> Result<(PathBuf, Report, Vec<Value>)> {
    super::timing::now(&request.worker, &request.id, "validation_started")?;
    let evidence = evidence(state, request)?;
    super::timing::now(&request.worker, &request.id, "validated")?;
    Ok(evidence)
}
fn timed_decision(
    home: &Path,
    request: &super::state::Request,
    actions: &[Value],
    report: &Report,
) -> Result<(judge::Judgment, Value, Value)> {
    super::timing::now(&request.worker, &request.id, "judgment_started")?;
    let result = decide(home, request, actions, report)?;
    super::timing::now(&request.worker, &request.id, "judged")?;
    Ok(result)
}

fn retry_correction(store: Store, worker: &str) -> Result<()> {
    drop(store);
    super::hook::spawn("__review-retry", worker)
}
