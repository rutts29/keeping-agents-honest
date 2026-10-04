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
    let store = Store::open_wait(worker)?;
    let mut state = store.load()?.ok_or("REVIEW_STATE_MISSING stage=consume")?;
    let Some(pending) = state.pending.clone() else {
        return Ok(());
    };
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
    let (path, report, actions) = evidence(&state, &request)?;
    let home = PathBuf::from(env::var_os("HOME").ok_or("REVIEW_HOME stage=consume")?);
    let decision = decide(&home, &request, &actions);
    if decision
        .as_ref()
        .is_err_and(|e| e.to_string().starts_with("JEV_PREPARATION_PENDING"))
    {
        drop(store);
        return super::runtime::prepare_context(worker);
    }
    let (judgment, input, assessment) = decision?;
    let tokens = tokens(&home, &store, &state);
    let result = json!({"request":request.id,"purpose":request.purpose,"head":request.head,"observer_session":state.observer.session,"pane":state.observer.pane,"report":report,"jev_state":input,"judgment":judgment,"verified_question_set":kah_observer::verified_judgment::VERSION,"assessment":assessment,"native_tokens":tokens.as_ref().ok().zip(pending.native_tokens_before).map(|(n,before)|n.saturating_sub(before)),"usage_gap":tokens.as_ref().err().map(ToString::to_string).or_else(||pending.native_tokens_before.is_none().then(||"REVIEW_USAGE_BASELINE_MISSING".into())),"elapsed_ms":kah_observer::session::now_ms().saturating_sub(pending.created_ms)});
    archive(&store.directory, &request.id, &result)?;
    if request.purpose == Purpose::Drift {
        crate::unattended_report::save(
            &store.directory,
            "last-drift-review.json",
            &json!({"request":request.id,"snapshot_created_ms":request.created_ms,"completed_ms":kah_observer::session::now_ms()}),
        )?;
    }
    record(worker, &request, &input, &judgment)?;
    if request.purpose == Purpose::Checkpoint {
        super::checkpoint::record(&store, &mut state, &request, &result)?;
    }
    let confirmed_steering = assessment["state"] == "steering_reviewed"
        && assessment["application"] == "verified_for_next_native_event"
        && matches!(judgment.answers.get("is_steering"),Some(judge::Answer::Noul{noul}) if *noul>=kah_observer::judge::questions::STEERING_PROBABILITY);
    if assessment["state"] == "material_drift" || confirmed_steering {
        super::reminder::save(&store, &request, &report)?;
    }
    state.latest = Some(result);
    super::redelivery::completed(&store, &request.id)?;
    state.pending = None;
    state.last_verified_turn = Some(request.turn.clone());
    if let Ok(tokens) = tokens {
        state.native_tokens = tokens;
    }
    store.save(&state)?;
    if request.purpose == Purpose::Checkpoint {
        super::checkpoint::notify(&state, &request, state.latest.as_ref().unwrap())?;
    }
    finish(store, &path, worker)
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
    let report: Report = serde_json::from_slice(&read_report(&path)?)?;
    if report.reminder.is_some() {
        super::reminder::text(state, request, &report)?;
    }
    let actions = report
        .verify(&request.id, &request.documents)?
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
) -> Result<(judge::Judgment, Value, Value)> {
    if request.purpose == Purpose::Checkpoint {
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
    let store = Store::open(worker)?;
    if let Some(notice) = super::redelivery::take(&store)? {
        return Ok(Some(notice));
    }
    let path = store.directory.join("reminder.json");
    let Some(mut reminder) = super::state::read::<Value>(&path)? else {
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
) -> Result<()> {
    use kah_observer::session;
    let mut primary = session::Store::open_wait(
        &kah_core::run_store::RunStore::default_state_root()?,
        worker,
    )?;
    let mut source = primary
        .load()?
        .ok_or("REVIEW_PRIMARY_STATE stage=consume")?;
    primary.append(&json!({"kind":"judgment","source":"verified_observer","purpose":request.purpose,"review_request":request.id,"prepared":super::preparation::pending(worker,&request.id)?,"judgment":judgment,"question_set":kah_observer::verified_judgment::VERSION,"at_ms":session::now_ms(),"state_digest":kah_observer::context_selection::digest(input)}))?;
    source.judgments.push(judgment.clone());
    source.usage.calls += judgment.metrics.calls;
    source.usage.input_tokens += judgment.metrics.input_tokens;
    source.usage.output_tokens += judgment.metrics.output_tokens;
    source.usage.elapsed_ms += judgment.metrics.elapsed_ms;
    primary.save(&source)?;
    Ok(())
}
