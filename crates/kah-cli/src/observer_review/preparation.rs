//! Persistent native context preparation; the model selects, Jev decides.
use super::state::{Request, Store};
use crate::Result;
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
    Ok(job_path(worker, decision)?.exists())
}
pub fn evaluate(
    home: &Path,
    request: &Request,
    input: &Value,
    questions: &Questions,
) -> Result<(judge::Judgment, Value)> {
    let binding = kah_observer::session::Store::read(
        &kah_core::run_store::RunStore::default_state_root()?,
        &request.worker,
    )?
    .ok_or("PREPARATION_BINDING stage=context_preparation")?
    .binding;
    let saved: Option<Job> = super::state::read(&job_path(&request.worker, &request.id)?)?;
    if let Some(job) = saved {
        return retry(home, request, &binding, &job);
    }
    let calibration = calibration(&request.worker)?;
    if let Some(plan) = calibration
        .and_then(|c| c.plan(input, questions))
        .filter(|p| p.prepare)
    {
        stage(request, input, questions, &serde_json::to_string(&plan)?)?;
        return Err("JEV_PREPARATION_PENDING stage=context_preparation".into());
    }
    match judge::routing::evaluate(home, binding.judge, &binding.jev_model, input, questions) {
        Ok(judgment) => {
            calibrate(&request.worker, input, questions, &judgment)?;
            Ok((judgment, input.clone()))
        }
        Err(error) if error.code == "JEV_CONTEXT_LIMIT" => {
            stage(request, input, questions, &error.to_string())?;
            Err("JEV_PREPARATION_PENDING stage=context_preparation".into())
        }
        Err(error) => Err(error.into()),
    }
}
fn selection_path(worker: &str, decision: &str) -> Result<PathBuf> {
    Ok(Store::directory(worker)?.join(format!("selection-{decision}.json")))
}
pub fn stage(request: &Request, input: &Value, questions: &Questions, reason: &str) -> Result<()> {
    let ids = request.intent["items"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|item| item["id"].as_str().map(str::to_owned))
        .collect::<BTreeSet<_>>();
    let job = Job {
        decision: request.id.clone(),
        worker: request.worker.clone(),
        state: input.clone(),
        questions: questions.clone(),
        items: context_selection::items(input, &ids),
        reason: reason.into(),
    };
    crate::unattended_report::save(
        &Store::directory(&request.worker)?,
        &format!("preparation-{}.json", request.id),
        &serde_json::to_value(job)?,
    )
}
pub fn prompt(worker: &str) -> Result<String> {
    let state = Store::read(worker)?.ok_or("PREPARATION_STATE stage=context_preparation")?;
    let pending = state
        .pending
        .as_ref()
        .ok_or("PREPARATION_PENDING stage=context_preparation")?;
    let job: Job = super::state::read(&job_path(worker, &pending.id)?)?
        .ok_or("PREPARATION_JOB stage=context_preparation")?;
    let directory = state.observer.workspace.join("inputs").join(&pending.id);
    let file = directory.join("preparation.json");
    fs::write(&file, serde_json::to_vec_pretty(&job)?)?;
    kah_run::access::share_tree(&directory, kah_run::access::service_uid()?, false)?;
    let output = state
        .observer
        .workspace
        .join("outbox")
        .join(format!("selection-{}.json", pending.id));
    let mut command = std::process::Command::new(state.observer.workspace.join("bin/kah"));
    command
        .args(["observe", "select", "--request"])
        .arg(&file)
        .arg("--json")
        .arg(&output);
    Ok(format!(
        "kah review request {} (Preparation). Prepare context for ONE unchanged Jev decision. Read {} and all original full context files in this input directory. Preparation reason: {}. Jev's documented token budgets are 32000 for state plus the longest question and 64000 for the whole request; no tokenizer is available, so do not treat bytes as a limit. Select WHOLE relevant items covering every question, keeping the goal and ALL active user/relayed items. No splitting, summaries of user words or text cuts. Write a manifest object with decision, selected, excluded; every item must appear exactly once. Each entry has id, digest, reason, superseded_by (null unless verified). Exclude only irrelevant non-user items; explain why. Never supply scores or a verdict. Submit using {} and become idle.",
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
    let job: Job = serde_json::from_slice(&fs::read(&args[3])?)?;
    let manifest: Manifest =
        serde_json::from_slice(&super::consume::read_report(Path::new(&args[5]))?)?;
    context_selection::prepare(
        &job.decision,
        &job.state,
        &job.items,
        &manifest,
        &BTreeMap::new(),
    )?;
    let root = Path::new(&args[3])
        .parent()
        .and_then(Path::parent)
        .and_then(Path::parent)
        .ok_or("PREPARATION_PATH stage=context_preparation")?;
    crate::unattended_report::save(
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
    let state = Store::read(worker)?.ok_or("PREPARATION_STATE stage=context_preparation")?;
    let path = state
        .observer
        .workspace
        .join("outbox")
        .join(format!("selected-{decision}.json"));
    if !path.exists() {
        return Ok(false);
    }
    let manifest: Manifest = serde_json::from_slice(&super::consume::read_report(&path)?)?;
    let job: Job = super::state::read(&job_path(worker, decision)?)?
        .ok_or("PREPARATION_JOB stage=context_preparation")?;
    context_selection::prepare(
        &job.decision,
        &job.state,
        &job.items,
        &manifest,
        &BTreeMap::new(),
    )?;
    crate::unattended_report::save(
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
    crate::unattended_report::save(
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
    crate::unattended_report::save(
        &Store::directory(worker)?,
        "preparation-notice.json",
        &json!({"decision":job.decision,"text":"kah decision used observer-prepared whole context; the selection manifest is archived."}),
    )?;
    crate::unattended_report::save(
        &Store::directory(worker)?,
        &format!("prepared-{}.json", job.decision),
        &json!({"decision":job.decision,"original":job.state,"questions":job.questions,"selection":manifest,"prepared_state":prepared,"judgment":judgment,"provider_tokens":judgment.metrics.input_tokens}),
    )
}

fn retry(
    home: &Path,
    request: &Request,
    binding: &kah_observer::session::Binding,
    job: &Job,
) -> Result<(judge::Judgment, Value)> {
    let selected = selection_path(&request.worker, &request.id)?;
    if !selected.exists() {
        return Err("JEV_PREPARATION_PENDING stage=context_preparation".into());
    }
    let manifest: Manifest = serde_json::from_slice(&super::consume::read_report(&selected)?)?;
    let prepared = context_selection::prepare(
        &job.decision,
        &job.state,
        &job.items,
        &manifest,
        &BTreeMap::new(),
    )?;
    let archive_path =
        Store::directory(&request.worker)?.join(format!("prepared-{}.json", request.id));
    if let Some(saved) = super::state::read::<Value>(&archive_path)? {
        if saved["selection"] != serde_json::to_value(&manifest)?
            || saved["prepared_state"] != prepared
        {
            return Err("PREPARATION_COMPLETED_IDENTITY stage=context_preparation".into());
        }
        return Ok((serde_json::from_value(saved["judgment"].clone())?, prepared));
    }
    crate::unattended_report::save(
        &Store::directory(&request.worker)?,
        &format!("preparation-retry-{}.json", request.id),
        &json!({"decision":request.id,"selection":manifest,"prepared_state":prepared,"questions":job.questions,"at_ms":kah_observer::session::now_ms()}),
    )?;
    let result = judge::routing::evaluate(
        home,
        binding.judge,
        &binding.jev_model,
        &prepared,
        &job.questions,
    );
    if let Err(error) = &result
        && error.code == "JEV_CONTEXT_LIMIT"
    {
        revise(request, job, &error.to_string())?;
        return Err("JEV_PREPARATION_PENDING stage=context_preparation".into());
    }
    let judgment = result?;
    archive(&request.worker, job, &manifest, &prepared, &judgment)?;
    calibrate(&request.worker, &prepared, &job.questions, &judgment)?;
    Ok((judgment, prepared))
}

fn revise(request: &Request, job: &Job, reason: &str) -> Result<()> {
    let mut updated = job.clone();
    updated.reason = format!(
        "Previous whole-item selection still exceeds the provider token budget: {reason}. Reconsider irrelevant items; protected authority cannot be omitted."
    );
    let directory = Store::directory(&request.worker)?;
    crate::unattended_report::save(
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
    Ok(())
}

fn calibration(worker: &str) -> Result<Option<kah_observer::context_budget::Calibration>> {
    let mut calibration: Option<kah_observer::context_budget::Calibration> =
        super::state::read(&Store::directory(worker)?.join("token-calibration.json"))?;
    if calibration.is_none() {
        let source = kah_observer::session::Store::read(
            &kah_core::run_store::RunStore::default_state_root()?,
            worker,
        )?;
        calibration = source.and_then(|s| {
            s.judgments.iter().rev().find_map(|j| {
                j.metrics
                    .input_utf8_bytes
                    .map(|bytes| kah_observer::context_budget::Calibration {
                        input_utf8_bytes: bytes,
                        input_tokens: j.metrics.input_tokens,
                    })
            })
        });
    }
    Ok(calibration)
}
