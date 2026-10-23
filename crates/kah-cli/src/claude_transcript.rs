//! Prefer a validated native host-hook pointer over a derived storage key.
use crate::Result;
use kah_core::run_store::{RunStore, ensure_private_directory};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    env, fs,
    path::{Component, Path, PathBuf},
};

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Saved {
    session: String,
    project: PathBuf,
    transcript: PathBuf,
}
pub fn register_hook(project: &Path, input: &Value) -> Result<()> {
    let home = PathBuf::from(env::var_os("HOME").ok_or("CLAUDE_HOME stage=hook")?);
    register(&home, project, input)
}

pub fn validate(home: &Path, session: &str, path: &Path) -> Result<()> {
    let root = kah_claude::paths::config_root(home).join("projects");
    if session.is_empty()
        || !session
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || c == b'-' || c == b'_')
        || !path.is_absolute()
        || !path.starts_with(&root)
        || path
            .components()
            .any(|c| matches!(c, Component::ParentDir | Component::CurDir))
        || path.file_name().and_then(|s| s.to_str()) != Some(&format!("{session}.jsonl"))
    {
        return Err(
            "RUN_NATIVE_TRANSCRIPT stage=native_binding: path or session outside native storage"
                .into(),
        );
    }
    if root.exists() {
        let actual_root = root.canonicalize()?;
        let mut ancestor = path;
        while !ancestor.exists() {
            ancestor = ancestor
                .parent()
                .ok_or("RUN_NATIVE_TRANSCRIPT stage=native_binding")?;
        }
        if !ancestor.canonicalize()?.starts_with(actual_root) {
            return Err(
                "RUN_NATIVE_TRANSCRIPT stage=native_binding: symlink escaped native storage".into(),
            );
        }
    }
    Ok(())
}
pub fn register(home: &Path, project: &Path, input: &Value) -> Result<()> {
    if input.get("agent_id").is_some() {
        return Ok(());
    }
    let session = input["session_id"]
        .as_str()
        .ok_or("RUN_NATIVE_ID stage=native_binding")?;
    let cwd = Path::new(
        input["cwd"]
            .as_str()
            .ok_or("RUN_NATIVE_CWD stage=native_binding")?,
    );
    if !cwd.starts_with(project) || cwd.components().any(|c| matches!(c, Component::ParentDir)) {
        return Err("RUN_NATIVE_SCOPE stage=native_binding".into());
    }
    let path = PathBuf::from(
        input["transcript_path"]
            .as_str()
            .ok_or("RUN_NATIVE_TRANSCRIPT stage=native_binding")?,
    );
    validate(home, session, &path)?;
    let directory = directory(home, session)?;
    ensure_private_directory(&directory)?;
    crate::unattended_report::save(
        &directory,
        "native-transcript.json",
        &serde_json::to_value(Saved {
            session: session.into(),
            project: project.into(),
            transcript: path,
        })?,
    )
}
pub fn resolve(
    home: &Path,
    project: &Path,
    session: &str,
    confirmed: Option<&Path>,
) -> Result<PathBuf> {
    if let Some(path) = confirmed {
        validate(home, session, path)?;
        return Ok(path.into());
    }
    let file = directory(home, session)?.join("native-transcript.json");
    let exists = match fs::symlink_metadata(&file) {
        Ok(_) => true,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => false,
        Err(error) => {
            return Err(format!("RUN_NATIVE_TRANSCRIPT_READ stage=run_history: {error}").into());
        }
    };
    if exists {
        let saved: Saved = crate::relay_store::read(&file)?;
        if saved.session != session || saved.project != project {
            return Err("RUN_NATIVE_SCOPE stage=run_history".into());
        }
        validate(home, session, &saved.transcript)?;
        return Ok(saved.transcript);
    }
    let path = kah_claude::paths::transcript(home, project, session);
    validate(home, session, &path)?;
    Ok(path)
}
fn directory(home: &Path, session: &str) -> Result<PathBuf> {
    Ok(kah_observer::session::Store::directory(
        &RunStore::state_root_for(home),
        session,
    ))
}
