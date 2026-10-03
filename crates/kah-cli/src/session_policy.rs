//! Host-owned session policy. This boundary stays active with --no-observer.
use kah_core::run_store::RunStore;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::Write,
    os::unix::fs::OpenOptionsExt,
    path::{Path, PathBuf},
};

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct SessionPolicy {
    pub project: PathBuf,
    pub policy: String,
    pub overlay: PathBuf,
}

impl SessionPolicy {
    pub fn check(&self, saved: &Self) -> Result<(), String> {
        if self == saved {
            return Ok(());
        }
        Err(format!(
            "SESSION_POLICY_MISMATCH stage=resume: reopen with kah {} {}{}",
            saved.policy.split('-').next().unwrap_or("<domain>"),
            saved
                .project
                .file_name()
                .unwrap_or_default()
                .to_string_lossy(),
            if saved.policy.ends_with("-online") {
                " --online"
            } else if saved.policy.ends_with("-yolo") {
                " --yolo"
            } else {
                ""
            }
        ))
    }
}

pub fn enforce(input: &Value) -> Result<(), String> {
    let encoded = std::env::var("KAH_SESSION_POLICY")
        .map_err(|_| "SESSION_POLICY_MISSING stage=hook: reopen through kah")?;
    let current: SessionPolicy = serde_json::from_str(&encoded)
        .map_err(|e| format!("SESSION_POLICY_JSON stage=hook: {e}"))?;
    let base = RunStore::default_state_root().map_err(|e| e.to_string())?;
    validate(&base, &current, input)
}

pub fn rejection(input: &Value, reason: &str) -> Value {
    if input["hook_event_name"] == "PreToolUse" {
        // 0.160.0 rejects continue:false at PreToolUse; use its native deny.
        return json!({"hookSpecificOutput":{"hookEventName":"PreToolUse","permissionDecision":"deny","permissionDecisionReason":reason},"systemMessage":reason});
    }
    // Universal fields are accepted by SessionStart and UserPromptSubmit.
    // SessionStart rejects the legacy decision/reason fields as invalid JSON.
    json!({"continue":false,"stopReason":reason,"systemMessage":reason})
}

pub fn saved(base: &Path, session: &str) -> Result<Option<SessionPolicy>, String> {
    let path = base
        .join("session-policies")
        .join(format!("{:x}.json", Sha256::digest(session.as_bytes())));
    match fs::read(path) {
        Ok(bytes) => serde_json::from_slice(&bytes)
            .map(Some)
            .map_err(|e| format!("SESSION_POLICY_JSON stage=resume: {e}")),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(io_error(e)),
    }
}

pub fn validate(base: &Path, current: &SessionPolicy, input: &Value) -> Result<(), String> {
    let session = input["session_id"]
        .as_str()
        .filter(|s| !s.is_empty())
        .ok_or("SESSION_POLICY_ID stage=hook: native session ID absent")?;
    let directory = base.join("session-policies");
    fs::create_dir_all(&directory).map_err(io_error)?;
    let path = directory.join(format!("{:x}.json", Sha256::digest(session.as_bytes())));
    let saved = match fs::read(&path) {
        Ok(bytes) => serde_json::from_slice(&bytes)
            .map_err(|e| format!("SESSION_POLICY_JSON stage=hook: {e}"))?,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => initial_policy(current, input)?,
        Err(e) => return Err(io_error(e)),
    };
    current.check(&saved)?;
    if current.policy.starts_with("research-") || current.policy.starts_with("untrusted-") {
        kah_codex::overlay::validate_executor(&current.overlay, &format!("kah-{}", current.policy))
            .map_err(|e| e.to_string())?;
    }
    let cwd = input["cwd"]
        .as_str()
        .ok_or("SESSION_POLICY_CWD stage=hook: cwd absent")?;
    if !Path::new(cwd)
        .canonicalize()
        .map_err(io_error)?
        .starts_with(&current.project)
    {
        return Err(format!(
            "SESSION_POLICY_CWD stage=resume: reopen with kah {} {}",
            current.policy.split('-').next().unwrap_or("<domain>"),
            current.project.display()
        ));
    }
    match fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&path)
    {
        Ok(mut file) => {
            file.write_all(&serde_json::to_vec(&saved).unwrap())
                .map_err(io_error)?;
            file.sync_all().map_err(io_error)?;
        }
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => (),
        Err(e) => return Err(io_error(e)),
    }
    Ok(())
}

fn initial_policy(current: &SessionPolicy, input: &Value) -> Result<SessionPolicy, String> {
    if input["hook_event_name"] == "SessionStart" && input["source"] == "startup" {
        return Ok(current.clone());
    }
    // Gate 2 rollouts predate the policy store; their stable kah overlay path
    // supplies the original policy. Never infer opt-ins from a project file.
    let transcript = input["transcript_path"].as_str().map(Path::new);
    if transcript.is_some_and(|p| p.starts_with(current.overlay.join("sessions"))) {
        return Ok(current.clone());
    }
    Err("SESSION_POLICY_UNKNOWN stage=resume: saved policy differs or is unavailable; reopen with kah <domain> <project>".into())
}

fn io_error(error: std::io::Error) -> String {
    format!("SESSION_POLICY_IO stage=hook: {error}")
}
