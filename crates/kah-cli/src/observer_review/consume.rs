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
    let file = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(path)?;
    let meta = file.metadata()?;
    if !meta.is_file() || meta.len() > 65536 {
        return Err("REVIEW_REPORT_BOUND stage=consume".into());
    }
    let mut bytes = vec![];
    file.take(65537).read_to_end(&mut bytes)?;
    if bytes.len() > 65536 {
        return Err("REVIEW_REPORT_BOUND stage=consume".into());
    }
    Ok(bytes)
}
pub fn run(worker: &str) -> Result<()> {
    super::runtime::require_active(worker)?;
    let store = Store::open(worker)?;
    let mut state = store.load()?.ok_or("REVIEW_STATE_MISSING stage=consume")?;
    let Some(pending) = state.pending.clone() else {
        return Ok(());
    };
    let path = state
        .observer
        .workspace
        .join("outbox")
        .join(format!("submitted-{}.json", pending.id));
    let request = workspace::request(&store, &pending.id)?;
    let report: Report = serde_json::from_slice(&read_report(&path)?)?;
    let verified = report.verify(&pending.id, &request.documents)?;
    if serde_json::to_vec(&verified)?.len() > 128 * 1024 {
        return Err("REVIEW_VERIFIED_BOUND stage=consume: reduce duplicate citations".into());
    }
    let actions = verified
        .into_iter()
        .enumerate()
        .map(|(i, v)| json!({"id":format!("verified-{i}"),"kind":"verifiedEvidence","evidence":v}))
        .collect::<Vec<_>>();
    let home = PathBuf::from(env::var_os("HOME").ok_or("REVIEW_HOME stage=consume")?);
    let (judgment, input, assessment) = decide(&home, &request, &actions)?;
    let tokens = tokens(&home, &store, &state);
    let result = json!({"request":request.id,"purpose":request.purpose,"head":request.head,"observer_session":state.observer.session,"pane":state.observer.pane,"report":report,"jev_state":input,"judgment":judgment,"verified_question_set":kah_observer::verified_judgment::VERSION,"assessment":assessment,"native_tokens":tokens.as_ref().ok().zip(pending.native_tokens_before).map(|(n,before)|n.saturating_sub(before)),"usage_gap":tokens.as_ref().err().map(ToString::to_string).or_else(||pending.native_tokens_before.is_none().then(||"REVIEW_USAGE_BASELINE_MISSING".into())),"elapsed_ms":kah_observer::session::now_ms().saturating_sub(pending.created_ms)});
    archive(&store.directory, &request.id, &result)?;
    if request.purpose == Purpose::Checkpoint {
        super::checkpoint::record(&store, &mut state, &request, &result)?;
    }
    if assessment["state"] == "material_drift" {
        crate::unattended_report::save(
            &store.directory,
            "reminder.json",
            &json!({"request":request.id,"turn":request.turn,"purpose":request.purpose,"text":format!("[kah verified observer; context, not user steering] {} Evidence: {}",assessment["reason"].as_str().unwrap_or("Review the verified evidence."),serde_json::to_string(&report)?)}),
        )?;
    }
    state.latest = Some(result);
    state.pending = None;
    state.last_verified_turn = Some(request.turn.clone());
    if let Ok(tokens) = tokens {
        state.native_tokens = tokens;
    }
    store.save(&state)?;
    if request.purpose == Purpose::Checkpoint {
        super::checkpoint::notify(&state, &request, state.latest.as_ref().unwrap())?;
    }
    fs::remove_file(path)?;
    let gap = store.directory.join("gap.json");
    if gap.exists() {
        fs::remove_file(gap)?;
    }
    drop(store);
    if super::events::pending(worker)? {
        super::hook::spawn("__checkpoint-worker", worker)?;
    }
    Ok(())
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
    kah_core::run_store::ensure_private_directory(&results)?;
    crate::unattended_report::save(&results, &format!("{request}.json"), result)
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
    let judgment = judge(&request.worker, home, &input)?;
    let failures = request.intent["failures"].as_array().map_or(0, Vec::len);
    let assessment = serde_json::to_value(kah_observer::verified_judgment::assess(
        &judgment, actions, failures,
    ))?;
    Ok((judgment, input, assessment))
}
fn judge(worker: &str, home: &Path, input: &Value) -> Result<judge::Judgment> {
    let binding = kah_observer::session::Store::read(
        &kah_core::run_store::RunStore::default_state_root()?,
        worker,
    )?
    .ok_or("REVIEW_WORKER_STATE stage=consume")?
    .binding;
    Ok(judge::routing::evaluate(
        home,
        binding.judge,
        &binding.jev_model,
        input,
        &kah_observer::verified_judgment::questions(),
    )?)
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
    let path = store.directory.join("reminder.json");
    let Some(reminder) = super::state::read::<Value>(&path)? else {
        return take_gap(&store);
    };
    fs::remove_file(path)?;
    if reminder["purpose"] == "drift" && reminder["turn"] != turn {
        return Ok(None);
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
