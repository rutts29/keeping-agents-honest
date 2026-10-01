//! One bounded detached judge per session; native hooks never wait for it.
use crate::Result;
use kah_core::run_store::RunStore;
use kah_observer::session::{Binding, Store};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    fs::{self, File, OpenOptions},
    os::unix::{
        fs::{OpenOptionsExt, PermissionsExt},
        process::CommandExt,
    },
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Job {
    binding: Binding,
    input: Value,
    actions: Vec<Value>,
    claude: bool,
}
pub fn schedule(
    binding: &Binding,
    input: &Value,
    claude: bool,
) -> std::result::Result<Value, String> {
    let base = RunStore::default_state_root().map_err(|e| e.to_string())?;
    crate::observer_hook::initialize(&base, binding)?;
    let directory = Store::directory(&base, &binding.session);
    let Some(lock) = lease(&directory)? else {
        return Ok(json!({}));
    };
    let output = take(&directory, input)?;
    let Some(actions) = crate::in_turn::batch(&base, binding, input)? else {
        return Ok(output);
    };
    // Keep only routing fields. Completed tool evidence was already scrubbed.
    let input = [
        "session_id",
        "prompt_id",
        "turn_id",
        "cwd",
        "transcript_path",
        "hook_event_name",
        "tool_use_id",
    ]
    .into_iter()
    .filter_map(|key| input.get(key).map(|v| (key.to_string(), v.clone())))
    .collect::<serde_json::Map<_, _>>();
    let job = Job {
        binding: binding.clone(),
        input: input.into(),
        actions,
        claude,
    };
    crate::unattended_report::save(
        &directory,
        "tool-job.json",
        &serde_json::to_value(job).unwrap(),
    )
    .map_err(|e| e.to_string())?;
    let mut command = Command::new("/usr/bin/timeout");
    command
        .args(["--kill-after=2s", "90s"])
        .arg(std::env::current_exe().map_err(failure)?)
        .arg("__in-turn-worker")
        .arg(directory.join("tool-job.json"))
        .stdin(Stdio::from(lock))
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .process_group(0);
    command.spawn().map_err(failure)?;
    Ok(output)
}

/// Also avoid contention at PreToolUse/Stop while the background judge owns the
/// observer store. Native events will reconcile fresh history on the next wake.
pub fn pending(binding: &Binding, input: &Value) -> std::result::Result<Option<Value>, String> {
    let base = RunStore::default_state_root().map_err(|e| e.to_string())?;
    let directory = Store::directory(&base, &binding.session);
    if !directory.exists() {
        return Ok(None);
    }
    let Some(_lock) = lease(&directory)? else {
        if input["hook_event_name"] == "UserPromptSubmit" {
            crate::unattended_report::save(&directory, "tool-new-user.json", &json!(true))
                .map_err(|e| e.to_string())?;
        }
        return Ok(Some(json!({})));
    };
    let output = take(&directory, input)?;
    Ok((output != json!({})).then_some(output))
}

pub fn run(args: &[String]) -> Result<()> {
    if args.len() != 2 {
        return Err("OBSERVER_WORKER_ARGS stage=in_turn".into());
    }
    let path = PathBuf::from(&args[1]);
    let job: Job =
        serde_json::from_value(read(&path)?.ok_or("OBSERVER_WORKER_JOB stage=in_turn")?)?;
    let base = RunStore::default_state_root()?;
    let directory = Store::directory(&base, &job.binding.session);
    if path != directory.join("tool-job.json") {
        return Err("OBSERVER_WORKER_SCOPE stage=in_turn".into());
    }
    let result = crate::in_turn::judge(&job.binding, &job.input, job.actions, job.claude);
    let output = match result {
        Ok(outcome) => crate::in_turn::output(&job.input, outcome),
        Err(error) => json!({"systemMessage":format!("kah observer gap: {error}")}),
    };
    crate::unattended_report::save(&directory, "tool-result.json", &output)?;
    fs::remove_file(path)?;
    Ok(())
}

fn lease(directory: &Path) -> std::result::Result<Option<File>, String> {
    let path = directory.join("tool-worker.lock");
    if fs::symlink_metadata(&path)
        .is_ok_and(|m| !m.is_file() || m.permissions().mode() & 0o077 != 0)
    {
        return Err("OBSERVER_WORKER_PRIVATE stage=in_turn".into());
    }
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .mode(0o600)
        .open(path)
        .map_err(failure)?;
    match file.try_lock() {
        Ok(()) => Ok(Some(file)),
        Err(fs::TryLockError::WouldBlock) => Ok(None),
        Err(fs::TryLockError::Error(e)) => Err(failure(e)),
    }
}
fn take(directory: &Path, input: &Value) -> std::result::Result<Value, String> {
    let newer_user = directory.join("tool-new-user.json");
    let discard = read(&newer_user)?.is_some();
    if discard {
        fs::remove_file(newer_user).map_err(failure)?;
    }
    let path = directory.join("tool-result.json");
    let Some(mut output) = read(&path)? else {
        if directory.join("tool-job.json").exists() {
            fs::remove_file(directory.join("tool-job.json")).map_err(failure)?;
            return Ok(
                json!({"systemMessage":"kah observer gap: OBSERVER_WORKER_INTERRUPTED stage=in_turn"}),
            );
        }
        return Ok(json!({}));
    };
    fs::remove_file(path).map_err(failure)?;
    if discard || input["hook_event_name"] == "UserPromptSubmit" {
        return Ok(json!({})); // New human input takes precedence over a stale reminder.
    }
    if output.get("hookSpecificOutput").is_some() {
        if input["hook_event_name"] == "Stop" {
            output = json!({"decision":"block","reason":output["hookSpecificOutput"]["additionalContext"]});
        } else {
            output["hookSpecificOutput"]["hookEventName"] = input["hook_event_name"].clone();
        }
    }
    Ok(output)
}
fn read(path: &Path) -> std::result::Result<Option<Value>, String> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(m) => m,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(failure(e)),
    };
    if !metadata.is_file()
        || metadata.permissions().mode() & 0o077 != 0
        || metadata.len() > 64 * 1024
    {
        return Err("OBSERVER_WORKER_PRIVATE stage=in_turn".into());
    }
    serde_json::from_slice(&fs::read(path).map_err(failure)?)
        .map(Some)
        .map_err(|_| "OBSERVER_WORKER_JSON stage=in_turn".into())
}
fn failure(error: std::io::Error) -> String {
    format!("OBSERVER_WORKER_IO stage=in_turn: {error}")
}
