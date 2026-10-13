//! A validated native ACK releases the observer slot while durable host work runs.
use super::{
    source_store,
    state::{Pending, Purpose, Request, State, Store},
};
use crate::Result;
use kah_observer::{judge::Judgment, review_evidence::Report};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    fs,
    path::{Path, PathBuf},
};
mod evaluation;
mod revision;
pub use evaluation::run;

#[derive(Clone, Serialize, Deserialize)]
pub struct Job {
    pub request: Request,
    pub state: State,
    pub pending: Pending,
    pub report: Report,
    pub actions: Vec<Value>,
    pub submitted: PathBuf,
    #[serde(default)]
    pub report_revision: Option<super::report_revision::Submission>,
    pub native_tokens_end: Option<u64>,
    #[serde(default)]
    pub usage_gap: Option<String>,
}
pub fn directory(worker: &str, request: &str) -> Result<PathBuf> {
    if !request.starts_with("review-")
        || !request
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-')
    {
        return Err("REVIEW_GATE_HOST_ID stage=handoff".into());
    }
    Ok(Store::directory(worker)?.join("gate-host").join(request))
}
/// Requires the held main store lock; saves the ACK-owned job before yielding.
pub fn handoff_locked(
    store: &Store,
    state: &mut State,
    request: &Request,
    report: &Report,
    actions: &[Value],
    submitted: &super::report_revision::Submission,
    native_tokens_end: Result<u64>,
) -> Result<bool> {
    if request.purpose != Purpose::Checkpoint || super::commit_packet::is_commit(request) {
        return Ok(false);
    }
    let pending = state
        .pending
        .clone()
        .filter(|p| p.id == request.id)
        .ok_or("REVIEW_GATE_HOST_PENDING stage=handoff")?;
    submitted.require_current()?;
    if !super::liveness::handoff::awaiting_host(state)?
        && !revision::owns_ack(state, request, &submitted.path)?
    {
        return Err(
            "REVIEW_SUBMISSION_UNCONFIRMED stage=gate_handoff: native ACK not validated".into(),
        );
    }
    let root = directory(&request.worker, &request.id)?;
    kah_core::run_store::ensure_private_directory(&root)?;
    let job = captured(
        state,
        pending,
        request,
        report,
        actions,
        submitted,
        native_tokens_end,
    );
    save_handoff_locked(store, &root, job)?;
    state.pending = None;
    store.save(state)?;
    marker(&root, "stage.json", "evaluating")?;
    Ok(true)
}
fn captured(
    state: &State,
    pending: Pending,
    request: &Request,
    report: &Report,
    actions: &[Value],
    submitted: &super::report_revision::Submission,
    native_tokens_end: Result<u64>,
) -> Job {
    let usage_gap = native_tokens_end
        .as_ref()
        .err()
        .map(ToString::to_string)
        .or_else(|| {
            pending
                .native_tokens_before
                .is_none()
                .then(|| "REVIEW_USAGE_BASELINE_MISSING".into())
        });
    Job {
        request: request.clone(),
        state: state.clone(),
        pending,
        report: report.clone(),
        actions: actions.to_vec(),
        submitted: submitted.path.clone(),
        report_revision: Some(submitted.clone()),
        usage_gap,
        native_tokens_end: native_tokens_end.ok(),
    }
}
fn save_handoff_locked(store: &Store, root: &Path, job: Job) -> Result<()> {
    if store.directory != Store::directory(&job.request.worker)? {
        return Err("REVIEW_GATE_HOST_BINDING stage=handoff".into());
    }
    let _driver = Store::delivery_at(&root.join("driver"), false).map_err(|e| {
        if e.to_string().starts_with("REVIEW_BUSY") {
            "STORE_BUSY stage=gate_revision: active host driver".into()
        } else {
            e
        }
    })?;
    revision::require_current(&job)?;
    let saved = if let Some(mut old) = super::state::read::<Job>(&root.join("job.json"))? {
        if serde_json::to_value(&old.request)? != serde_json::to_value(&job.request)?
            || old.state.observer.session != job.state.observer.session
        {
            return Err("REVIEW_GATE_HOST_CHANGED stage=handoff".into());
        }
        if revision::changed(&old, &job)? {
            revision::archive(root, &old)?;
            return source_store::save(root, "job.json", &serde_json::to_value(&job)?);
        }
        old.report_revision = job.report_revision.clone().or(old.report_revision);
        // Continuation preserves the initial baseline/report. Only cumulative
        // native usage advances after another selection ACK in the same session.
        old.native_tokens_end = old.native_tokens_end.max(job.native_tokens_end);
        old.usage_gap = job.usage_gap.or_else(|| {
            old.pending
                .native_tokens_before
                .is_none()
                .then(|| "REVIEW_USAGE_BASELINE_MISSING".into())
        });
        old
    } else {
        job
    };
    source_store::save(root, "job.json", &serde_json::to_value(&saved)?)
}
pub fn jobs(worker: &str) -> Result<Vec<PathBuf>> {
    let root = Store::directory(worker)?.join("gate-host");
    if !root.is_dir() {
        return Ok(vec![]);
    }
    let mut jobs = fs::read_dir(root)?
        .map(|e| e.map(|e| e.path()))
        .collect::<std::io::Result<Vec<_>>>()?;
    jobs.retain(|p| p.join("job.json").is_file() && !p.join("finished.json").exists());
    jobs.sort();
    Ok(jobs)
}
pub fn parent(worker: &str, decision: &str) -> Result<Option<String>> {
    for path in jobs(worker)? {
        let review = path.file_name().unwrap().to_str().unwrap();
        if decision == review
            || decision
                .strip_prefix(review)
                .is_some_and(|s| s.starts_with('-'))
        {
            return Ok(Some(review.into()));
        }
    }
    Ok(None)
}
pub fn runnable(worker: &str) -> Result<bool> {
    for path in jobs(worker)? {
        if ready(&path)? {
            return Ok(true);
        }
    }
    Ok(false)
}
fn ready(root: &Path) -> Result<bool> {
    let stage: Option<Value> = super::state::read(&root.join("stage.json"))?;
    Ok(stage.is_none_or(|s| s["stage"] == "evaluating"))
}
pub fn marker(root: &Path, name: &str, stage: &str) -> Result<()> {
    crate::unattended_report::save(
        root,
        name,
        &json!({"stage":stage,"at_ms":kah_observer::session::now_ms()}),
    )
}
/// Opens its own main store lock and merges only into the freshly loaded state.
pub fn merge_fresh(
    job: &Job,
    judgment: &Judgment,
    input: &Value,
    assessment: &Value,
) -> Result<Value> {
    let root = directory(&job.request.worker, &job.request.id)?;
    let store = Store::open_wait(&job.request.worker)?;
    if let Some(result) = super::state::read::<Value>(&root.join("finished.json"))? {
        return Ok(result);
    }
    let mut state = store
        .load()?
        .ok_or("REVIEW_STATE_MISSING stage=gate_merge")?;
    if state.observer.session != job.state.observer.session {
        return Err("REVIEW_GATE_HOST_OBSERVER stage=gate_merge: observer identity changed".into());
    }
    revision::require_current(job)?;
    let mut assessment = assessment.clone();
    super::gate_approval::guard_report(
        &store.directory,
        &job.request,
        &job.report,
        &mut assessment,
    )?;
    let result = json!({"request":job.request.id,"purpose":job.request.purpose,"head":job.request.head,
        "observer_session":job.state.observer.session,"pane":job.state.observer.pane,"report":job.report,
        "jev_state":input,"judgment":judgment,"assessment":assessment,
        "verified_question_set":super::consume::question_version(&job.request,&job.report),
        "native_tokens":job.native_tokens_end.zip(job.pending.native_tokens_before).map(|(n,b)|n.saturating_sub(b)),
        "usage_gap":job.usage_gap,"report_revision":job.report_revision,
        "elapsed_ms":kah_observer::session::now_ms().saturating_sub(job.pending.created_ms)});
    super::consume::finish_gate_host(&store, &mut state, job, &result, input, judgment)?;
    source_store::save(&root, "finished.json", &result)?;
    marker(&root, "stage.json", "finished")?;
    Ok(result)
}

#[cfg(test)]
#[path = "../../tests/observer_integration/gate_interleaving.rs"]
mod tests;

#[cfg(test)]
#[path = "../../tests/observer_unit/report_revision.rs"]
mod revision_tests;
