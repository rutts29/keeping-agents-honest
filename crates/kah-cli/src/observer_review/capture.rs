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
mod rendering;
pub use rendering::{native_document, render};

pub fn request(state: &State, purpose: Purpose, turn: &str, trigger: Value) -> Result<Request> {
    let home = PathBuf::from(std::env::var_os("HOME").ok_or("REVIEW_HOME stage=context")?);
    let source = Store::read(&RunStore::default_state_root()?, &state.worker.session)?
        .ok_or("REVIEW_WORKER_STATE stage=context")?;
    let projection = engine::projection(&source)?;
    let intent=projection.as_ref().map(|p|json!({"revision":p.revision,"goal":p.goal.text,"goal_source":p.goal.source_refs,"items":p.preserve.iter().chain(&p.frontier).chain(&p.validation).map(|i|json!({"id":i.id,"text":i.text,"source":i.introduced_by})).collect::<Vec<_>>(),"failures":source.failures,"completed_outcomes":[],"open_outcomes":[],"open_deferrals":state.deferrals})).unwrap_or(Value::Null);
    let mut documents = vec![Document::new(
        "ledger",
        "ledger",
        "kah:ledger",
        &serde_json::to_string_pretty(&projection)?,
    )];
    documents.push(history(&home, state)?);
    let mut project = project_context(&home, state)?;
    let mut trigger = trigger;
    if purpose == Purpose::Drift {
        trigger["last_completed_drift_review"] = super::state::read::<Value>(
            &super::state::Store::directory(&state.worker.session)?.join("last-drift-review.json"),
        )?
        .unwrap_or(Value::Null);
    }
    project_documents(&mut project, &trigger, &mut documents)?;
    add_previous(state, &mut documents)?;
    add_reconsideration(state, &trigger, &mut documents)?;
    add_questions(state, &mut documents)?;
    let id = format!(
        "review-{}-{}",
        kah_observer::session::now_ms(),
        std::process::id()
    );
    Ok(Request {
        id,
        worker: state.worker.session.clone(),
        purpose,
        turn: turn.into(),
        head: project["head"].as_str().map(str::to_owned),
        intent,
        trigger,
        documents,
        created_ms: kah_observer::session::now_ms(),
    })
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
    documents.push(Document::new(
        "committed-snapshot",
        "committed_code",
        "git:HEAD-complete-tree",
        project["committed_snapshot"].as_str().unwrap_or(""),
    ));
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
        "diff",
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

fn history(home: &Path, state: &State) -> Result<Document> {
    let directory = super::state::Store::directory(&state.worker.session)?;
    let mut history = History::open(
        home,
        &directory,
        &state.worker.provider,
        &state.worker.project,
        &state.worker.session,
    )?;
    let mut value = match &mut history {
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
    native_document(&state.worker.provider, &state.worker.session, &value)
}

fn project_context(home: &Path, state: &State) -> Result<Value> {
    let output = if state.worker.project.starts_with(home.join("repos/mine")) {
        // Mine already authorizes native host tools. Keep run worktrees and
        // restricted repositories behind the existing gVisor Git boundary.
        Command::new(home.join(".local/bin/kah"))
            .arg("__review-project-context")
            .current_dir(&state.worker.project)
            .output()?
    } else {
        restricted_context(home, state)?
    };
    decode_project(output)
}
fn restricted_context(home: &Path, state: &State) -> Result<std::process::Output> {
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
    Ok(executor
        .command(
            &project.root,
            &[
                binary.to_string_lossy().into_owned(),
                "__review-project-context".into(),
            ],
        )?
        .output()?)
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
    if record.worktree != state.worker.project
        || record.native.as_ref().and_then(|n| n.session.as_deref()) != Some(&state.worker.session)
    {
        return Err("REVIEW_WORKTREE_BINDING stage=context".into());
    }
    Ok(Some(
        kah_run::worktree::Worktree::load(
            &kah_run::store::Store::path(&base, id)?.join("worktree.json"),
        )?
        .git_dir,
    ))
}

/// Runs only in the read-only gVisor context collector. No model or credentials.
pub fn project_cli() -> Result<()> {
    let root = std::env::current_dir()?;
    let head = git(&root, &["rev-parse", "--verify", "HEAD"])?;
    let committed = committed_snapshot(&root, head.as_deref())?;
    let diff = git(
        &root,
        &["diff", "--no-ext-diff", "--no-textconv", "--binary"],
    )?;
    let staged = git(
        &root,
        &[
            "diff",
            "--cached",
            "--no-ext-diff",
            "--no-textconv",
            "--binary",
        ],
    )?;
    let status = git(
        &root,
        &["status", "--porcelain=v1", "--untracked-files=all"],
    )?;
    let tags = git(&root, &["show-ref", "--tags"])?;
    if root.join(".git").exists() && (diff.is_none() || staged.is_none() || status.is_none()) {
        return Err("REVIEW_GIT_INCOMPLETE stage=context: cannot read complete Git context".into());
    }
    let mut plans = vec![];
    for path in [
        "docs/PLAN.md",
        "docs/STATUS.md",
        "docs/HANDOFF.md",
        "PLAN.md",
        "AGENTS.md",
        "CLAUDE.md",
    ] {
        match fs::read_to_string(root.join(path)) {
            Ok(text) => plans.push(json!({"path":path,"text":text})),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => (),
            Err(e) => return Err(format!("REVIEW_PLAN_READ stage=context: {path}: {e}").into()),
        }
    }
    println!(
        "{}",
        json!({"head":head.map(|h|h.trim().to_owned()),"committed_snapshot":committed,"diff":format!("{}\n{}",diff.unwrap_or_default(),staged.unwrap_or_default()),"status":status,"tags":tags,"plans":plans,"full_context_not_truncated":true})
    );
    Ok(())
}
fn committed_snapshot(root: &Path, head: Option<&str>) -> Result<String> {
    let Some(head) = head else {
        return Ok(String::new());
    };
    // Diffing the empty tree captures ALL earlier phases, even on a clean tree.
    // hash-object without -w computes the native SHA-1/SHA-256 empty tree ID
    // without writing an object or authoring a commit.
    let empty = git(root, &["hash-object", "-t", "tree", "/dev/null"])?
        .ok_or("REVIEW_EMPTY_TREE stage=context")?;
    git(
        root,
        &[
            "diff",
            "--no-ext-diff",
            "--no-textconv",
            "--binary",
            empty.trim(),
            head.trim(),
            "--",
        ],
    )?
    .ok_or_else(|| {
        "REVIEW_COMMITTED_CONTEXT stage=context: cannot read cumulative HEAD snapshot".into()
    })
}
fn git(root: &Path, args: &[&str]) -> Result<Option<String>> {
    let output = Command::new("/usr/bin/git")
        .env_clear()
        .env("PATH", "/usr/bin:/bin")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_NO_REPLACE_OBJECTS", "1")
        .current_dir(root)
        .args([
            "-c",
            "core.hooksPath=/dev/null",
            "-c",
            "core.fsmonitor=false",
            "-c",
            "core.pager=cat",
            "-c",
        ])
        .arg(format!("safe.directory={}", root.display()))
        .args(args)
        .output()?;
    if !output.status.success() {
        eprintln!(
            "REVIEW_GIT_COMMAND stage=context: git {args:?}: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    Ok(output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).into_owned()))
}
