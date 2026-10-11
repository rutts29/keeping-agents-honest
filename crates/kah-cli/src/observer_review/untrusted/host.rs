//! Hooks publish metadata; the detached host job owns Git reads and Jev.
use super::super::state::{self, Store};
use super::{Report, notice};
use crate::Result;
use kah_core::run_store::RunStore;
use kah_observer::session::{Binding, Store as Sessions};
use serde_json::{Value, json};
use std::{
    env, fs,
    path::{Path, PathBuf},
};

pub fn scope(binding: &Binding) -> Result<bool> {
    let home = PathBuf::from(env::var_os("HOME").ok_or("UNTRUSTED_HOME stage=scope")?);
    if binding.project.starts_with(home.join("repos/untrusted")) {
        return Ok(true);
    }
    let Some(id) = binding
        .project
        .parent()
        .and_then(|p| p.file_name())
        .and_then(|p| p.to_str())
        .filter(|s| s.starts_with("run-"))
    else {
        return Ok(false);
    };
    let record = kah_run::store::Store::read(&RunStore::default_state_root()?.join("runs"), id)?;
    Ok(record.domain == "untrusted" && record.worktree == binding.project)
}
pub fn root(worker: &str) -> Result<PathBuf> {
    Ok(Store::directory(worker)?.join("untrusted"))
}
pub fn schedule(binding: &Binding, input: &Value) -> Result<()> {
    if !scope(binding)?
        || kah_observer::session::paused(&RunStore::default_state_root()?, &binding.session)
    {
        return Ok(());
    }
    if !matches!(
        input["hook_event_name"].as_str(),
        Some("SessionStart" | "PostToolUse" | "PostToolUseFailure" | "Stop")
    ) {
        return Ok(());
    }
    let directory = root(&binding.session)?.join("events");
    kah_core::run_store::ensure_private_directory(&directory)?;
    let fields = ["session_id", "cwd", "transcript_path", "hook_event_name"]
        .into_iter()
        .filter_map(|k| input.get(k).map(|v| (k.to_owned(), v.clone())))
        .collect::<serde_json::Map<_, _>>();
    crate::unattended_report::save(
        &directory,
        &format!("{}.json", crate::relay_store::nonce()),
        &Value::Object(fields),
    )?;
    super::super::hook::spawn("__untrusted-commits", &binding.session)
}
pub fn take(binding: &Binding, input: &Value) -> Result<Value> {
    if !scope(binding)?
        || !matches!(
            input["hook_event_name"].as_str(),
            Some("SessionStart" | "PostToolUse" | "UserPromptSubmit")
        )
    {
        return Ok(json!({}));
    }
    take_at(
        &root(&binding.session)?,
        input["hook_event_name"].as_str().unwrap(),
    )
}
pub fn take_at(root: &Path, event: &str) -> Result<Value> {
    if !matches!(event, "SessionStart" | "PostToolUse" | "UserPromptSubmit") {
        return Ok(json!({}));
    }
    let _lock = Store::delivery_at(&root.join("delivery"), true)?;
    for path in entries(&root.join("notices"))? {
        let report: Report =
            state::read(&path)?.ok_or("UNTRUSTED_NOTICE_MISSING stage=delivery")?;
        let done = root.join("delivered");
        if done.join(path.file_name().unwrap()).exists() {
            continue;
        }
        let output = notice(&report, event)?;
        kah_core::run_store::ensure_private_directory(&done)?;
        crate::unattended_report::save(
            &done,
            path.file_name().unwrap().to_str().unwrap(),
            &json!({"head":report.record.head,"event":event,"native_wake_count":0}),
        )?;
        return Ok(output);
    }
    Ok(json!({}))
}
pub fn entries(directory: &Path) -> Result<Vec<PathBuf>> {
    if !directory.exists() {
        return Ok(vec![]);
    }
    let mut paths = fs::read_dir(directory)?
        .map(|e| e.map(|e| e.path()))
        .collect::<std::io::Result<Vec<_>>>()?;
    paths.retain(|p| p.extension().is_some_and(|s| s == "json"));
    paths.sort();
    Ok(paths)
}
pub fn summary(worker: &str) -> Result<Value> {
    let root = root(worker)?;
    if !root.exists() {
        return Ok(Value::Null);
    }
    let reports = entries(&root.join("results"))?
        .into_iter()
        .map(|p| state::read::<Report>(&p))
        .collect::<Result<Vec<_>>>()?
        .into_iter()
        .flatten()
        .collect::<Vec<_>>();
    let pending: Option<Value> = state::read(&root.join("pending.json"))?;
    Ok(json!({"domain":"untrusted","pending":pending,"reports":reports,"native_wake_count":0}))
}
pub fn binding(worker: &str) -> Result<Binding> {
    Ok(Sessions::read(&RunStore::default_state_root()?, worker)?
        .ok_or("UNTRUSTED_BINDING stage=commit")?
        .binding)
}
