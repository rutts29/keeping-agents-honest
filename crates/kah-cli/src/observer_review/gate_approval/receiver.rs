//! Explicit native user answers persist before any review-lock wait. No model
//! interprets approve/reject; ordinary intent intake still retains the words.
use super::super::{capture, hook, state::Store};
use super::*;
use kah_core::{intent_history::SourceRef, projection::ActiveProjection, run_store::RunStore};
use kah_observer::{engine, session};
use serde::{Deserialize, Serialize};
use std::{fs, path::PathBuf};

#[derive(Serialize, Deserialize)]
struct Answer {
    source: SourceRef,
    text: String,
    revision: u64,
    authority: Value,
}
pub fn enqueue_answer(worker: &str, source: &SourceRef, text: &str) -> Result<()> {
    if source.question_answer.is_none() {
        return Ok(());
    }
    source.validate()?;
    let primary = session::Store::read(&RunStore::default_state_root()?, worker)?
        .ok_or("REVIEW_PRIMARY_STATE stage=approval")?;
    let projection = engine::projection(&primary)?.ok_or("REVIEW_PRIMARY_INTENT stage=approval")?;
    let root = Store::directory(worker)?.join("gate-answers");
    kah_core::run_store::ensure_private_directory(&root)?;
    super::super::source_store::save(
        &root,
        &format!("{}.json", crate::relay_store::nonce()),
        &serde_json::to_value(Answer {
            source: source.clone(),
            text: text.into(),
            revision: projection.revision,
            authority: authority(&projection, None),
        })?,
    )?;
    hook::spawn("__gate-answers", worker)
}
fn authority(p: &ActiveProjection, own: Option<&Answer>) -> Value {
    let own_source = |s: &SourceRef| own.is_some_and(|a| s.record == a.source.record);
    let goal = if p.goal.source_refs.iter().any(own_source) {
        own.unwrap().authority["goal"].clone()
    } else {
        json!({"text":p.goal.text,"sources":p.goal.source_refs})
    };
    let mut items = p
        .preserve
        .iter()
        .chain(&p.frontier)
        .chain(&p.validation)
        .filter(|i| !i.introduced_by.iter().any(own_source))
        .map(|i| json!({"id":i.id,"text":i.text,"sources":i.introduced_by}))
        .collect::<Vec<_>>();
    items.sort_by(|a, b| a["id"].as_str().cmp(&b["id"].as_str()));
    json!({"goal":goal,"items":items})
}
pub fn apply_answers(worker: &str) -> Result<()> {
    let store = Store::open_wait(worker)?;
    let mut state = store.load()?.ok_or("REVIEW_STATE_MISSING stage=approval")?;
    if state.pending.is_some() {
        return Ok(());
    }
    let root = store.directory.join("gate-answers");
    if !root.exists() {
        return Ok(());
    }
    let done = store.directory.join("gate-answers-applied");
    kah_core::run_store::ensure_private_directory(&done)?;
    let mut paths = fs::read_dir(&root)?
        .map(|e| e.map(|e| e.path()))
        .collect::<std::io::Result<Vec<_>>>()?;
    paths.sort();
    for path in paths
        .into_iter()
        .filter(|p| p.extension().is_some_and(|e| e == "json"))
    {
        let answer: Answer =
            super::super::state::read(&path)?.ok_or("REVIEW_GATE_ANSWER_MISSING stage=approval")?;
        match apply(&mut state, &answer) {
            Ok(()) => store.save(&state)?,
            Err(error) => rejected(&store.directory, &done, &path, &answer, &error.to_string())?,
        }
        fs::rename(&path, done.join(path.file_name().unwrap()))?;
    }
    Ok(())
}
pub fn pending_answers(worker: &str) -> Result<bool> {
    let root = Store::directory(worker)?.join("gate-answers");
    if !root.exists() {
        return Ok(false);
    }
    for entry in fs::read_dir(root)? {
        if entry?.path().extension().is_some_and(|e| e == "json") {
            return Ok(true);
        }
    }
    Ok(false)
}
fn rejected(root: &Path, done: &Path, path: &Path, answer: &Answer, error: &str) -> Result<()> {
    let record = json!({"answer":answer,"error":error,"at_ms":session::now_ms()});
    super::super::source_store::save(
        done,
        &format!("error-{}", path.file_name().unwrap().to_str().unwrap()),
        &record,
    )?;
    super::super::reminder_queue::save(
        root,
        &json!({"purpose":"uncertainty","request":"gate-answer","turn":"user-answer",
        "system_notice":true,"text":format!("kah could not apply user gate answer {}: {error}. Original words/receipt are retained; the gate remains waiting.",answer.source.question_answer.as_ref().unwrap().question)}),
    )
}
fn apply(state: &mut super::super::state::State, answer: &Answer) -> Result<()> {
    let base = RunStore::default_state_root()?;
    let primary = session::Store::open_wait(&base, &state.worker.session)?;
    let snapshot = primary
        .load()?
        .ok_or("REVIEW_PRIMARY_STATE stage=approval")?;
    let projection =
        engine::projection(&snapshot)?.ok_or("REVIEW_PRIMARY_INTENT stage=approval")?;
    if authority(&projection, Some(answer)) != answer.authority {
        return Err(
            "REVIEW_GATE_ANSWER_STALE stage=approval: user authority changed after answer arrival"
                .into(),
        );
    }
    let home = PathBuf::from(std::env::var_os("HOME").ok_or("REVIEW_HOME stage=approval")?);
    let project = capture::project_context(&home, state)?;
    let head = project["head"]
        .as_str()
        .ok_or("REVIEW_GATE_HEAD_MISSING stage=approval")?;
    let violations = confirmed_violations(state);
    let root = Store::directory(&state.worker.session)?;
    if state
        .checkpoint
        .as_ref()
        .is_none_or(|c| c["head"] != head || c["intent_revision"] != answer.revision)
    {
        return Err("REVIEW_GATE_ANSWER_STALE stage=approval: checkpoint binding changed".into());
    }
    let question_id = &answer.source.question_answer.as_ref().unwrap().question;
    let candidate: Value =
        super::super::state::read(&root.join("questions").join(format!("{question_id}.json")))?
            .ok_or("REVIEW_GATE_QUESTION_UNKNOWN stage=approval")?;
    same_checkpoint(state, &candidate)?;
    let question = resolve(
        &root,
        &answer.source,
        &answer.text,
        head,
        answer.revision,
        &violations,
    )?
    .ok_or("REVIEW_GATE_ANSWER_REQUIRED stage=approval")?;
    let checkpoint = state
        .checkpoint
        .as_mut()
        .ok_or("REVIEW_GATE_CHECKPOINT_MISSING stage=approval")?;
    let approved = question["resolution"]["decision"] == "approve";
    checkpoint["passed"] = approved.into();
    checkpoint["state"] = if approved {
        "checkpoint_user_approved"
    } else {
        "checkpoint_user_rejected"
    }
    .into();
    checkpoint["user_answer"] = question["resolution"].clone();
    checkpoint["pending_questions"] = json!([]);
    Ok(())
}
fn same_checkpoint(state: &super::super::state::State, question: &Value) -> Result<()> {
    if question["worker"] != state.worker.session
        || state.checkpoint.as_ref().is_none_or(|checkpoint| {
            ["request", "head", "intent_revision"]
                .iter()
                .any(|key| checkpoint[*key] != question[*key])
        })
    {
        return Err(
            "REVIEW_GATE_ANSWER_STALE stage=approval: question belongs to another checkpoint"
                .into(),
        );
    }
    Ok(())
}
fn confirmed_violations(state: &super::super::state::State) -> Vec<String> {
    [
        state.checkpoint.as_ref().map(|c| &c["violations"]),
        state
            .latest
            .as_ref()
            .map(|r| &r["assessment"]["violations"]),
    ]
    .into_iter()
    .flatten()
    .filter_map(Value::as_array)
    .flatten()
    .filter_map(Value::as_str)
    .map(str::to_owned)
    .collect::<std::collections::BTreeSet<_>>()
    .into_iter()
    .collect()
}

#[cfg(test)]
#[path = "../../../tests/observer_unit/checkpoint_violations.rs"]
mod tests;
