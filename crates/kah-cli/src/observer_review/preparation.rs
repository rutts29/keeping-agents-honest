//! Persistent native context preparation; the model selects, Jev decides.
use super::state::{Request, Store};
pub mod batch;
mod calibration;
pub mod cursors;
mod job;
pub mod rejection;
pub use job::job;
mod validation;
use crate::Result;
pub(crate) use calibration::calibration;
pub(crate) mod ready;
pub(crate) mod send;
use kah_observer::{
    context_selection::{self, Item, Manifest},
    judge::{self, Questions},
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
};
pub(crate) use validation::validate_selection;

#[derive(Clone, Serialize, Deserialize)]
pub struct Job {
    pub decision: String,
    pub worker: String,
    pub state: Value,
    pub questions: Questions,
    pub items: Vec<Item>,
    pub reason: String,
}
fn job_path(worker: &str, decision: &str) -> Result<PathBuf> {
    Ok(Store::directory(worker)?.join(format!("preparation-{decision}.json")))
}
pub fn pending(worker: &str, decision: &str) -> Result<bool> {
    if let Some(batch) = batch::load(worker, decision)? {
        return batch::pending(worker, &batch);
    }
    let root = Store::directory(worker)?;
    Ok(job_path(worker, decision)?.exists()
        && !root.join(format!("prepared-{decision}.json")).exists())
}
pub fn active_decision(worker: &str, review: &str) -> Result<String> {
    cursors::active(worker, review)
}
pub fn evaluate(
    home: &Path,
    request: &Request,
    input: &Value,
    questions: &Questions,
) -> Result<(judge::Judgment, Value)> {
    let calibration = calibration(&request.worker)?;
    ready::evaluated(
        home,
        ready::prepare(request, input, questions, calibration.as_ref())?,
    )
}
pub fn matches_decision(job: &Job, input: &Value, questions: &Questions) -> Result<bool> {
    Ok(job.state == *input
        && serde_json::to_value(&job.questions)? == serde_json::to_value(questions)?)
}
fn archive_changed(request: &Request) -> Result<()> {
    let directory = Store::directory(&request.worker)?;
    let history = directory
        .join("preparation-history")
        .join(crate::relay_store::nonce());
    kah_core::run_store::ensure_private_directory(&history)?;
    let state =
        Store::read(&request.worker)?.ok_or("PREPARATION_STATE stage=context_preparation")?;
    let mut paths = vec![];
    for prefix in [
        "preparation",
        "prepared",
        "selection",
        "preparation-woken",
        "preparation-retry",
    ] {
        paths.push(directory.join(format!("{prefix}-{}.json", request.id)));
    }
    paths.push(
        state
            .observer
            .workspace
            .join("outbox")
            .join(format!("selected-{}.json", request.id)),
    );
    for path in paths {
        if path.exists() {
            fs::rename(
                &path,
                history.join(
                    path.file_name()
                        .ok_or("PREPARATION_HISTORY_PATH stage=context_preparation")?,
                ),
            )?;
        }
    }
    Ok(())
}
fn selection_path(worker: &str, decision: &str) -> Result<PathBuf> {
    Ok(Store::directory(worker)?.join(format!("selection-{decision}.json")))
}
pub fn stage(request: &Request, input: &Value, questions: &Questions, reason: &str) -> Result<()> {
    let job = job(request, input, questions, reason);
    super::source_store::save(
        &Store::directory(&request.worker)?,
        &format!("preparation-{}.json", request.id),
        &serde_json::to_value(job)?,
    )?;
    let review = cursors::review(request)?;
    cursors::activate(&request.worker, &review, &request.id)
}
pub fn prompt(worker: &str) -> Result<String> {
    let state = Store::read(worker)?.ok_or("PREPARATION_STATE stage=context_preparation")?;
    let pending = state
        .pending
        .as_ref()
        .ok_or("PREPARATION_PENDING stage=context_preparation")?;
    let decision = active_decision(worker, &pending.id)?;
    super::context_input::ensure(&state.observer.workspace.join("inputs").join(&pending.id))?;
    if let Some(batch) = batch::load(worker, &decision)? {
        return batch::prompt(worker, &batch);
    }
    let job: Job = super::state::read(&job_path(worker, &decision)?)?
        .ok_or("PREPARATION_JOB stage=context_preparation")?;
    let directory = state.observer.workspace.join("inputs").join(&pending.id);
    let name = format!("preparation-{}.json", crate::relay_store::nonce());
    let file = directory.join(&name);
    super::source_store::save(&directory, &name, &serde_json::to_value(&job)?)?;
    super::source_store::share(&Store::directory(worker)?, &state.observer.workspace)?;
    kah_run::access::share_tree(&directory, kah_run::access::service_uid()?, false)?;
    let output = state
        .observer
        .workspace
        .join("outbox")
        .join(format!("selection-{decision}.json"));
    let mut command = std::process::Command::new(state.observer.workspace.join("bin/kah"));
    command
        .args(["observe", "select", "--request"])
        .arg(&file)
        .arg("--json")
        .arg(&output);
    Ok(format!(
        "kah review request {} (Preparation). Prepare context for ONE unchanged Jev decision. Read {} (the preparation job) and context-input.json (the reference catalogue). Read intent/delta first, then whole relevant records/sources on demand; all originals remain available. Do not reconstruct every input, full canonical request or transcript into outbox/context copies. Keep source descriptors by reference. Inputs use lossless source references. Run bin/kah observe read-source PATH for the preparation JSON or any source descriptor to reconstruct the complete original state. Digest-named sources may be segment manifests; never interpret a manifest as the original content. Preparation reason: {}. Jev's documented token budgets are 32000 for state plus the longest question and 64000 for the whole request; no tokenizer is available, so do not treat bytes as a limit. Select WHOLE relevant items covering every question, keeping the goal and ALL active user/relayed items. No splitting, summaries of user words or text cuts. Write a manifest object with decision, selected, excluded; every item must appear exactly once. Each entry has id, digest, reason, superseded_by (null unless verified). Exclude only irrelevant non-user items; explain why. Never supply scores or a verdict. Submit using {} in the foreground and require context_selection_submitted acknowledgement; finish any running submission tool before becoming idle. Do not start other work after acknowledgement.",
        job.decision,
        file.display(),
        job.reason,
        crate::native_run::shell_command(&command)?
    ))
}
pub fn submit(args: &[String]) -> Result<()> {
    if args.len() != 6 || args[2] != "--request" || args[4] != "--json" {
        return Err("PREPARATION_ARGUMENTS stage=context_preparation".into());
    }
    let job: Job = super::source_store::read(Path::new(&args[3]))?;
    let bytes = super::consume::read_report(Path::new(&args[5]))?;
    let root = Path::new(&args[3])
        .parent()
        .and_then(Path::parent)
        .and_then(Path::parent)
        .ok_or("PREPARATION_PATH stage=context_preparation")?;
    let manifest = match validate_selection(&job, &bytes) {
        Ok(manifest) => manifest,
        Err(error) => {
            super::liveness::selection_rejection::record(
                &root.join("outbox"),
                &job,
                Path::new(&args[5]),
                &bytes,
                error.as_ref(),
            )?;
            return Err(error);
        }
    };
    super::workspace::publish_output(
        &root.join("outbox"),
        &format!("selected-{}.json", job.decision),
        &serde_json::to_value(manifest)?,
    )?;
    println!(
        "{}",
        json!({"decision":job.decision,"status":"context_selection_submitted"})
    );
    Ok(())
}
pub fn accept(worker: &str, decision: &str) -> Result<bool> {
    if let Some(batch) = batch::load(worker, decision)? {
        return batch::accept(worker, &batch);
    }
    let state = Store::read(worker)?.ok_or("PREPARATION_STATE stage=context_preparation")?;
    let path = state
        .observer
        .workspace
        .join("outbox")
        .join(format!("selected-{decision}.json"));
    if !path.exists() {
        return Ok(false);
    }
    let bytes = super::consume::read_report(&path)?;
    let job: Job = super::state::read(&job_path(worker, decision)?)?
        .ok_or("PREPARATION_JOB stage=context_preparation")?;
    let manifest = validate_selection(&job, &bytes)?;
    super::source_store::save(
        &Store::directory(worker)?,
        &format!("selection-{decision}.json"),
        &serde_json::to_value(manifest)?,
    )?;
    Ok(true)
}
pub fn notice(worker: &str) -> Result<Option<String>> {
    let path = Store::directory(worker)?.join("preparation-notice.json");
    let value: Option<Value> = super::state::read(&path)?;
    let Some(value) = value else {
        return Ok(None);
    };
    fs::remove_file(path)?;
    Ok(value["text"].as_str().map(str::to_owned))
}
fn calibrate(
    worker: &str,
    state: &Value,
    questions: &Questions,
    judgment: &judge::Judgment,
) -> Result<()> {
    super::source_store::save(
        &Store::directory(worker)?,
        "token-calibration.json",
        &json!({"model":judgment.model,"input_utf8_bytes":state.to_string().len()+serde_json::to_vec(questions)?.len(),"input_tokens":judgment.metrics.input_tokens,"planning_only":true}),
    )
}
fn archive(
    worker: &str,
    job: &Job,
    manifest: &Manifest,
    prepared: &Value,
    judgment: &judge::Judgment,
) -> Result<()> {
    super::source_store::save(
        &Store::directory(worker)?,
        "preparation-notice.json",
        &json!({"decision":job.decision,"text":"kah decision used observer-prepared whole context; the selection manifest is archived."}),
    )?;
    super::source_store::save(
        &Store::directory(worker)?,
        &format!("prepared-{}.json", job.decision),
        &json!({"decision":job.decision,"original":job.state,"questions":job.questions,"selection":manifest,"prepared_state":prepared,"judgment":judgment,"provider_tokens":judgment.metrics.input_tokens}),
    )
}

fn revise(request: &Request, job: &Job, reason: &str) -> Result<()> {
    let mut updated = job.clone();
    updated.reason = format!(
        "Previous whole-item selection still exceeds the provider token budget: {reason}. Reconsider irrelevant items; protected authority cannot be omitted."
    );
    let directory = Store::directory(&request.worker)?;
    super::source_store::save(
        &directory,
        &format!("preparation-{}.json", request.id),
        &serde_json::to_value(updated)?,
    )?;
    let state =
        Store::read(&request.worker)?.ok_or("PREPARATION_STATE stage=context_preparation")?;
    for path in [
        selection_path(&request.worker, &request.id)?,
        directory.join(format!("preparation-woken-{}.json", request.id)),
        state
            .observer
            .workspace
            .join("outbox")
            .join(format!("selected-{}.json", request.id)),
    ] {
        if path.exists() {
            fs::remove_file(path)?;
        }
    }
    batch::reopen(&request.worker, &request.id)?;
    Ok(())
}
