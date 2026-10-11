//! Durable tool batches; one bounded detached judge, hooks never wait for Jev.
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
    if !crate::observer_intake::legacy_checks(binding)? {
        return Ok(json!({}));
    }
    let base = RunStore::default_state_root().map_err(|e| e.to_string())?;
    crate::observer_hook::initialize(&base, binding)?;
    let directory = Store::directory(&base, &binding.session);
    if let Some(actions) = crate::in_turn::batch(&base, binding, input)? {
        let fields = [
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
            input: fields.into(),
            actions,
            claude,
        };
        let jobs = directory.join("tool-jobs");
        kah_core::run_store::ensure_private_directory(&jobs).map_err(|e| e.to_string())?;
        crate::unattended_report::save(
            &jobs,
            &format!("{}.json", crate::relay_store::nonce()),
            &serde_json::to_value(job).unwrap(),
        )
        .map_err(|e| e.to_string())?;
    }
    let output = take(&directory, input)?;
    kick(&directory).map_err(|e| e.to_string())?;
    Ok(output)
}

pub fn pending(binding: &Binding, input: &Value) -> std::result::Result<Option<Value>, String> {
    if !crate::observer_intake::legacy_checks(binding)? {
        return Ok(None);
    }
    let directory = Store::directory(
        &RunStore::default_state_root().map_err(|e| e.to_string())?,
        &binding.session,
    );
    if !directory.exists() {
        return Ok(None);
    }
    let output = take(&directory, input)?;
    let Some(_lock) = lease(&directory)? else {
        if input["hook_event_name"] == "UserPromptSubmit" {
            crate::unattended_report::save(&directory, "tool-new-user.json", &json!(true))
                .map_err(|e| e.to_string())?;
        }
        return Ok(Some(output));
    };
    kick(&directory).map_err(|e| e.to_string())?;
    Ok((output != json!({})).then_some(output))
}

pub fn run(args: &[String]) -> Result<()> {
    if args.len() != 2 {
        return Err("OBSERVER_WORKER_ARGS stage=in_turn".into());
    }
    let directory = PathBuf::from(&args[1]);
    let Some(lock) = lease(&directory)? else {
        return Ok(());
    };
    let Some(path) = first(&directory.join("tool-jobs"))? else {
        return Ok(());
    };
    let job: Job =
        serde_json::from_value(read(&path)?.ok_or("OBSERVER_WORKER_JOB stage=in_turn")?)?;
    let base = RunStore::default_state_root()?;
    if directory != Store::directory(&base, &job.binding.session) {
        return Err("OBSERVER_WORKER_SCOPE stage=in_turn".into());
    }
    if !crate::observer_intake::legacy_checks(&job.binding)? {
        return Ok(());
    }
    let result = crate::in_turn::judge(&job.binding, &job.input, job.actions, job.claude);
    let output = match result {
        Ok(mut outcome) => {
            if let Err(error) =
                crate::observer_review::dispatch::outcome(&job.binding, &mut outcome)
            {
                outcome.gap = Some(error.to_string());
            }
            crate::in_turn::output(&job.input, outcome)
        }
        Err(error) => json!({"systemMessage":format!("kah observer gap: {error}")}),
    };
    let results = directory.join("tool-results");
    kah_core::run_store::ensure_private_directory(&results)?;
    crate::unattended_report::save(
        &results,
        path.file_name().unwrap().to_str().unwrap(),
        &json!({"turn":turn(&job.input),"output":output}),
    )?;
    fs::remove_file(path)?;
    drop(lock);
    kick(&directory)
}
fn kick(directory: &Path) -> Result<()> {
    if first(&directory.join("tool-jobs"))?.is_none() {
        return Ok(());
    }
    Command::new("/usr/bin/timeout")
        .args(["--kill-after=2s", "90s"])
        .arg(std::env::current_exe()?)
        .arg("__in-turn-worker")
        .arg(directory)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .process_group(0)
        .spawn()?;
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
    let newer = directory.join("tool-new-user.json");
    let previous = read(&newer)?.is_some() || input["hook_event_name"] == "UserPromptSubmit";
    if newer.exists() {
        fs::remove_file(newer).map_err(failure)?;
    }
    let path = first(&directory.join("tool-results"))
        .map_err(|e| e.to_string())?
        .unwrap_or_else(|| directory.join("tool-result.json"));
    let Some(saved) = read(&path)? else {
        return Ok(json!({}));
    };
    fs::remove_file(path).map_err(failure)?;
    let mut output = saved
        .get("output")
        .cloned()
        .unwrap_or_else(|| saved.clone());
    if let Some(context) = output["hookSpecificOutput"]["additionalContext"].as_str() {
        if previous || saved.get("turn").is_some_and(|t| *t != turn(input)) {
            output["hookSpecificOutput"]["additionalContext"]=format!("[kah confirmed finding from previous turn; retained context, not new user steering] {context}").into();
        }
        if input["hook_event_name"] == "Stop" {
            output = json!({"decision":"block","reason":output["hookSpecificOutput"]["additionalContext"]});
        } else {
            output["hookSpecificOutput"]["hookEventName"] = input["hook_event_name"].clone();
        }
    }
    Ok(output)
}
fn turn(input: &Value) -> Value {
    input
        .get("turn_id")
        .or_else(|| input.get("prompt_id"))
        .cloned()
        .unwrap_or(json!("session"))
}
fn first(directory: &Path) -> Result<Option<PathBuf>> {
    if !directory.exists() {
        return Ok(None);
    }
    let mut paths = fs::read_dir(directory)?
        .map(|e| e.map(|e| e.path()))
        .collect::<std::io::Result<Vec<_>>>()?;
    paths.retain(|p| p.extension().is_some_and(|e| e == "json"));
    paths.sort();
    Ok(paths.into_iter().next())
}
fn read(path: &Path) -> std::result::Result<Option<Value>, String> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(m) => m,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(failure(e)),
    };
    if !metadata.is_file() || metadata.permissions().mode() & 0o077 != 0 {
        return Err("OBSERVER_WORKER_PRIVATE stage=in_turn".into());
    }
    serde_json::from_slice(&fs::read(path).map_err(failure)?)
        .map(Some)
        .map_err(|_| "OBSERVER_WORKER_JSON stage=in_turn".into())
}
fn failure(error: std::io::Error) -> String {
    format!("OBSERVER_WORKER_IO stage=in_turn: {error}")
}
