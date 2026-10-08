use super::state::{Purpose, Request, State};
use crate::{Result, native_history::History};
use kah_core::run_store::RunStore;
use kah_observer::{engine, review_evidence::Document, session::Store};
use kah_sandbox::{Domain, LaunchOptions, Project};
use serde_json::{Value, json};
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};
mod binding;
mod project;
pub use project::cli as project_cli;
pub use project::cli_with_map as project_cli_with_map;
pub mod controller_events;
pub mod controller_receipts;
mod executions;
pub mod historical;
mod rendering;
use binding::binding;
pub use rendering::{native_document, render};

pub fn request(state: &State, purpose: Purpose, turn: &str, trigger: Value) -> Result<Request> {
    let mut trigger = super::commit_packet::trigger(&purpose, trigger);
    let home = PathBuf::from(std::env::var_os("HOME").ok_or("REVIEW_HOME stage=context")?);
    let source = Store::read(&RunStore::default_state_root()?, &state.worker.session)?
        .ok_or("REVIEW_WORKER_STATE stage=context")?;
    let projection = engine::projection_wait(&source)?;
    let intent=projection.as_ref().map(|p|json!({"revision":p.revision,"goal":p.goal.text,"goal_source":p.goal.source_refs,"items":p.preserve.iter().chain(&p.frontier).chain(&p.validation).map(|i|json!({"id":i.id,"text":i.text,"source":i.introduced_by})).collect::<Vec<_>>(),"failures":source.failures,"completed_outcomes":[],"open_outcomes":[],"open_deferrals":state.deferrals})).unwrap_or(Value::Null);
    let mut documents = vec![Document::new(
        "ledger",
        "ledger",
        "kah:ledger",
        &serde_json::to_string_pretty(&projection)?,
    )];
    documents.push(history(&home, state)?);
    let mut project = scoped_project_context(&home, state, &trigger)?;
    super::artifacts::documents(&state.worker.session, &project, &mut documents)?;
    trigger["decision_protocol_version"] = json!(kah_observer::decision_units::VERSION);
    if purpose == Purpose::Steering {
        let input = serde_json::from_value(trigger["uncertain_user_input"].clone())?;
        trigger["steering_context"] = super::steering::context(&state.worker.session, &input)?;
    }
    if purpose == Purpose::Drift || trigger["review_class"] == "commit" {
        trigger["last_completed_drift_review"] = super::state::read::<Value>(
            &super::state::Store::directory(&state.worker.session)?.join("last-drift-review.json"),
        )?
        .unwrap_or(Value::Null);
    }
    project_documents(&mut project, &trigger, &mut documents)?;
    super::commit_packet::persist(state, &mut documents)?;
    add_previous(state, &mut documents)?;
    add_reconsideration(state, &trigger, &mut documents)?;
    add_questions(state, &mut documents)?;
    let id = format!(
        "review-{}-{}",
        kah_observer::session::now_ms(),
        std::process::id()
    );
    let request = Request {
        id,
        worker: state.worker.session.clone(),
        purpose,
        turn: turn.into(),
        head: project["head"].as_str().map(str::to_owned),
        intent,
        trigger,
        documents,
        created_ms: kah_observer::session::now_ms(),
    };
    finish_request(state, request)
}
fn finish_request(state: &State, mut request: Request) -> Result<Request> {
    super::completion_due::add(&mut request)?;
    controller_events::add(
        &super::state::Store::directory(&request.worker)?,
        &mut request,
    )?;
    controller_receipts::add(&mut request)?;
    historical::add(
        &super::state::Store::directory(&request.worker)?,
        &mut request,
    )?;
    super::verdict_cache::plan::prepare(
        &super::state::Store::directory(&request.worker)?,
        &mut request,
    )?;
    super::packet_references::scope(
        &super::state::Store::directory(&request.worker)?,
        &state.observer.workspace,
        &mut request,
    )?;
    Ok(request)
}
fn add_questions(state: &State, documents: &mut Vec<Document>) -> Result<()> {
    let questions =
        super::questions::pending(&super::state::Store::directory(&state.worker.session)?)?;
    if !questions.is_empty() {
        documents.push(Document::new(
            "pending-questions",
            "controller_questions",
            "kah:pending-questions",
            &serde_json::to_string_pretty(&questions)?,
        ));
    }
    Ok(())
}
fn add_reconsideration(
    state: &State,
    trigger: &Value,
    documents: &mut Vec<Document>,
) -> Result<()> {
    let Some(id) = trigger["reconsideration"]["request"].as_str() else {
        return Ok(());
    };
    if !id.starts_with("review-") || !id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-') {
        return Err("REVIEW_REQUEST_ID stage=reconsideration".into());
    }
    let result: Value = super::state::read(
        &super::state::Store::directory(&state.worker.session)?
            .join("results")
            .join(format!("{id}.json")),
    )?
    .ok_or("REVIEW_RESULT_MISSING stage=reconsideration")?;
    documents.push(Document::new(
        "reconsidered-review",
        "prior_review",
        &format!("kah:{id}"),
        &serde_json::to_string_pretty(&result)?,
    ));
    Ok(())
}
fn project_documents(
    project: &mut Value,
    trigger: &Value,
    documents: &mut Vec<Document>,
) -> Result<()> {
    let commit = (trigger["review_class"] == "commit" || trigger["packet_scope"] == "diff-v1")
        && !project["head"].is_null();
    if commit {
        documents.extend(super::commit_packet::documents(project)?);
    } else {
        documents.push(Document::new(
            "committed-snapshot",
            "committed_code",
            "git:HEAD-complete-tree",
            project["committed_snapshot"].as_str().unwrap_or(""),
        ));
    }
    documents.push(Document::new(
        "checkpoint-event",
        "controller_event",
        "kah:review-trigger",
        &serde_json::to_string_pretty(&trigger)?,
    ));
    project
        .as_object_mut()
        .unwrap()
        .remove("committed_snapshot");
    documents.push(Document::new(
        if commit { "working-diff" } else { "diff" },
        "diff",
        "git:working-diff",
        project["diff"].as_str().unwrap_or(""),
    ));
    documents.push(Document::new(
        "git",
        "controller_facts",
        "git:context",
        &serde_json::to_string_pretty(&project)?,
    ));
    for (index, plan) in project["plans"].as_array().unwrap().iter().enumerate() {
        documents.push(Document::new(
            &format!("plan-{index}"),
            "plan",
            plan["path"].as_str().unwrap(),
            plan["text"].as_str().unwrap(),
        ));
    }
    Ok(())
}
fn scoped_project_context(home: &Path, state: &State, trigger: &Value) -> Result<Value> {
    if trigger["review_class"] != "commit" && trigger["packet_scope"] != "diff-v1" {
        return project_context(home, state);
    }
    if super::tags::head(home, state)?.is_none() {
        return project_context(home, state);
    }
    let previous = super::state::read::<Value>(
        &super::state::Store::directory(&state.worker.session)?.join("last-reviewed-sha.json"),
    )?;
    let baseline = previous
        .as_ref()
        .and_then(|p| p["head"].as_str())
        .or_else(|| state.checkpoint.as_ref().and_then(|r| r["head"].as_str()))
        .or_else(|| {
            state
                .latest
                .as_ref()
                .filter(|r| r["purpose"] == "checkpoint")
                .and_then(|r| r["head"].as_str())
        })
        .unwrap_or("empty");
    project_context_with_baseline(home, state, Some(baseline))
}
fn add_previous(state: &State, documents: &mut Vec<Document>) -> Result<()> {
    if let Some(previous) = &state.latest {
        documents.push(Document::new(
            "previous-review",
            "prior_review",
            "kah:previous-review",
            &serde_json::to_string_pretty(previous)?,
        ));
    }
    Ok(())
}

pub(super) fn history(home: &Path, state: &State) -> Result<Document> {
    let directory = super::state::Store::directory(&state.worker.session)?;
    let mut history = History::open(
        home,
        &directory,
        &state.worker.provider,
        &state.worker.project,
        &state.worker.session,
    )?;
    history_document(&mut history, &state.worker.provider, &state.worker.session)
}
pub(super) fn history_document(
    history: &mut History,
    provider: &str,
    worker: &str,
) -> Result<Document> {
    let mut value = match history {
        History::Codex {
            client, session, ..
        } => json!(kah_codex::source::full_history(client, session)?),
        History::Claude { path, .. } => {
            let bytes = fs::read(path)?;
            Value::Array(
                String::from_utf8(bytes)?
                    .lines()
                    .map(serde_json::from_str)
                    .collect::<std::result::Result<Vec<Value>, _>>()?,
            )
        }
    };
    crate::privacy::scrub(&mut value);
    native_document(provider, worker, &value)
}

pub(crate) fn project_context(home: &Path, state: &State) -> Result<Value> {
    project_context_with_baseline(home, state, None)
}
pub fn project_context_with_baseline(
    home: &Path,
    state: &State,
    baseline: Option<&str>,
) -> Result<Value> {
    let mut args = vec!["__review-project-context".to_owned()];
    let map_stamp = if baseline.is_some() {
        super::component_map::producer::current(
            &super::state::Store::directory(&state.worker.session)?,
            &state.worker.session,
        )?
    } else {
        None
    };
    if let Some(baseline) = baseline {
        args.extend(["--commit-baseline".into(), baseline.into()]);
        let map = super::component_map::producer::path(&state.observer.workspace);
        if map_stamp.is_some() {
            args.extend(["--component-map".into(), map.to_string_lossy().into_owned()]);
        }
    }
    let output = if state.worker.project.starts_with(home.join("repos/mine")) {
        // Mine already authorizes native host tools. Keep run worktrees and
        // restricted repositories behind the existing gVisor Git boundary.
        Command::new(home.join(".local/bin/kah"))
            .args(&args)
            .current_dir(&state.worker.project)
            .output()?
    } else {
        restricted_context(home, state, &args)?
    };
    let mut project = decode_project(output)?;
    if let Some(stamp) = &map_stamp {
        super::component_map::producer::check_collected(stamp, &project)?;
        project["component_map_binding"] = stamp.clone();
    }
    Ok(project)
}
fn restricted_context(home: &Path, state: &State, args: &[String]) -> Result<std::process::Output> {
    let project = Project {
        domain: Domain::Research,
        root: state.worker.project.clone(),
        relative_cwd: PathBuf::new(),
        needs_creation: false,
    };
    let git = worktree_git(state)?;
    let mut executor =
        crate::native_setup::executor(home, &project, LaunchOptions::default(), git.as_deref())?;
    let binary = state.observer.workspace.join("bin/kah");
    executor = executor
        .with_read_only_mounts(&[(binary.clone(), binary.clone())])?
        .read_only_project();
    let map = super::component_map::producer::path(&state.observer.workspace);
    if args.iter().any(|arg| arg == "--component-map") {
        executor = executor.with_read_only_mounts(&[(map.clone(), map)])?;
    }
    let mut command = vec![binary.to_string_lossy().into_owned()];
    command.extend_from_slice(args);
    Ok(executor.command(&project.root, &command)?.output()?)
}
fn decode_project(output: std::process::Output) -> Result<Value> {
    if !output.status.success() {
        return Err(format!(
            "REVIEW_PROJECT_READ stage=context: {}",
            String::from_utf8_lossy(&output.stderr)
        )
        .into());
    }
    let mut value: Value =
        serde_json::from_slice(&output.stdout).map_err(|_| "REVIEW_PROJECT_JSON stage=context")?;
    crate::privacy::scrub(&mut value);
    Ok(value)
}

pub(super) fn worktree_git(state: &State) -> Result<Option<PathBuf>> {
    let Some(id) = state
        .worker
        .project
        .parent()
        .and_then(|p| p.file_name())
        .and_then(|p| p.to_str())
        .filter(|s| s.starts_with("run-"))
    else {
        return Ok(None);
    };
    let base = RunStore::default_state_root()?.join("runs");
    let record = kah_run::store::Store::read(&base, id)?;
    binding(
        &state.worker.project,
        &record.worktree,
        record.native.as_ref().map(|n| n.session.as_deref()),
        &state.worker.session,
    )?;
    Ok(Some(
        kah_run::worktree::Worktree::load(
            &kah_run::store::Store::path(&base, id)?.join("worktree.json"),
        )?
        .git_dir,
    ))
}
