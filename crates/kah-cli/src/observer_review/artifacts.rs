//! Capture the baseline before new-session work; resumes never reset it.
use super::{source_store, state::Store};
use crate::Result;
use kah_observer::{
    decision_units::artifact::Receipt, review_evidence::Document, session::Binding,
};
use serde_json::{Value, json};
use std::{
    env,
    path::{Path, PathBuf},
    process::Command,
};

pub fn start_status(binding: &Binding, input: &Value) -> Result<Option<Value>> {
    if let Err(error) = start(binding, input) {
        let directory = Store::directory(&binding.session)?.join("artifact-baseline");
        source_store::save(
            &directory,
            "baseline.json",
            &json!({"initial":false,"reason":error.to_string()}),
        )?;
        return Ok(Some(
            json!({"systemMessage":format!("kah artifact baseline unavailable: {error}. Intent observation continues; preservation cannot be claimed from this missing baseline.")}),
        ));
    }
    Ok(None)
}
pub fn start(binding: &Binding, input: &Value) -> Result<()> {
    if input["hook_event_name"] != "SessionStart" {
        return Ok(());
    }
    let directory = Store::directory(&binding.session)?.join("artifact-baseline");
    let _lock = Store::delivery_at(&directory, true)?;
    if directory.join("baseline.json").exists() {
        return Ok(());
    }
    if input["source"] != "startup" {
        return source_store::save(
            &directory,
            "baseline.json",
            &json!({"initial":false,"reason":"no original pre-work baseline was captured for this resumed session"}),
        );
    }
    let home = PathBuf::from(env::var_os("HOME").ok_or("REVIEW_HOME stage=artifact_baseline")?);
    let output = if binding.project.starts_with(home.join("repos/mine")) {
        Command::new(home.join(".local/bin/kah"))
            .arg("__review-artifact-baseline")
            .current_dir(&binding.project)
            .output()?
    } else {
        restricted(&home, &binding.project)?
    };
    if !output.status.success() {
        return Err(format!(
            "REVIEW_ARTIFACT_BASELINE stage=artifact_baseline: {}",
            String::from_utf8_lossy(&output.stderr)
        )
        .into());
    }
    let mut baseline: Value = serde_json::from_slice(&output.stdout)?;
    baseline["initial"] = true.into();
    source_store::save(&directory, "baseline.json", &baseline)
}
fn restricted(home: &Path, root: &Path) -> Result<std::process::Output> {
    let binary = home.join(".local/bin/kah");
    let project = kah_sandbox::Project {
        domain: kah_sandbox::Domain::Research,
        root: root.into(),
        relative_cwd: Default::default(),
        needs_creation: false,
    };
    let git = run_git(root)?;
    let executor =
        crate::native_setup::executor(home, &project, Default::default(), git.as_deref())?
            .with_read_only_mounts(&[(binary.clone(), binary.clone())])?
            .read_only_project();
    Ok(executor
        .command(
            root,
            &[
                binary.to_string_lossy().into_owned(),
                "__review-artifact-baseline".into(),
            ],
        )?
        .output()?)
}
fn run_git(root: &Path) -> Result<Option<PathBuf>> {
    let Some(id) = root
        .parent()
        .and_then(|p| p.file_name())
        .and_then(|p| p.to_str())
        .filter(|s| s.starts_with("run-"))
    else {
        return Ok(None);
    };
    let base = kah_core::run_store::RunStore::default_state_root()?.join("runs");
    let record = kah_run::store::Store::read(&base, id)?;
    if record.worktree != root {
        return Err("REVIEW_ARTIFACT_WORKTREE stage=artifact_baseline".into());
    }
    Ok(Some(
        kah_run::worktree::Worktree::load(
            &kah_run::store::Store::path(&base, id)?.join("worktree.json"),
        )?
        .git_dir,
    ))
}
pub fn documents(worker: &str, project: &Value, documents: &mut Vec<Document>) -> Result<()> {
    let Some(baseline) = super::state::read::<Value>(
        &Store::directory(worker)?.join("artifact-baseline/baseline.json"),
    )?
    else {
        return Ok(());
    };
    if baseline["initial"] != true {
        documents.push(Document::new(
            "artifact-baseline-unavailable",
            "controller_facts",
            "kah:artifact-baseline",
            &serde_json::to_string(&baseline)?,
        ));
        return Ok(());
    }
    let Some(head) = project["head"].as_str() else {
        return Ok(());
    };
    let Some(blobs) = baseline["blobs"].as_object() else {
        return Err("REVIEW_ARTIFACT_BASELINE_JSON stage=artifact_context".into());
    };
    for (index, (path, blob)) in blobs.iter().enumerate() {
        let receipt = Receipt {
            path: path.clone(),
            baseline_blob: blob
                .as_str()
                .ok_or("REVIEW_ARTIFACT_BLOB stage=artifact_context")?
                .into(),
            delivered_blob: project["artifact_blobs"][path].as_str().map(str::to_owned),
            delivered_sha: head.into(),
        };
        documents.push(Document::new(
            &format!("artifact-{index}"),
            "controller_artifact_digests",
            &format!("git:{head}:{path}"),
            &serde_json::to_string(&receipt)?,
        ));
    }
    documents.push(Document::new(
        "artifact-baseline-inventory",
        "controller_facts",
        "kah:artifact-baseline",
        &serde_json::to_string(&baseline)?,
    ));
    Ok(())
}
