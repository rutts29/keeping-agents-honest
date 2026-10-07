//! Native host hook confirmation; Herdr's process-discovery ID is advisory.
use kah_run::{native::Confirmation, store::Store};
use serde_json::Value;
use std::{
    env,
    path::{Component, Path, PathBuf},
};

pub fn claude(input: &Value, project: &Path) -> Result<(), String> {
    let (Ok(id), Ok(instance)) = (env::var("KAH_RUN_ID"), env::var("KAH_RUN_INSTANCE")) else {
        return Ok(());
    };
    let home = PathBuf::from(env::var_os("HOME").ok_or("RUN_HOME stage=native_binding")?);
    let Some(confirmation) = claude_confirmation(input, &home, project, &instance)? else {
        return Ok(());
    };
    let base = kah_core::run_store::RunStore::default_state_root()
        .map_err(|e| e.to_string())?
        .join("runs");
    let mut store = Store::open_wait(&base, &id)?;
    let mut record = store.load()?;
    if record.worktree != project {
        return Err("RUN_NATIVE_SCOPE stage=native_binding".into());
    }
    let native = record
        .native
        .as_mut()
        .ok_or("RUN_NATIVE_STATE stage=native_binding")?;
    if native.instance.as_deref() != Some(&instance) {
        return Ok(());
    }
    if native.provider != "claude" {
        return Err("RUN_NATIVE_PROVIDER stage=native_binding".into());
    }
    native.confirmation = Some(confirmation);
    store.save(&record)
}

pub fn claude_confirmation(
    input: &Value,
    home: &Path,
    project: &Path,
    instance: &str,
) -> Result<Option<Confirmation>, String> {
    if input["hook_event_name"] != "SessionStart"
        || input.get("agent_id").is_some()
        || !matches!(
            input["source"].as_str(),
            Some("startup" | "resume" | "clear" | "compact" | "fork")
        )
    {
        return Ok(None);
    }
    let session = input["session_id"]
        .as_str()
        .ok_or("RUN_NATIVE_ID stage=native_binding")?;
    if session.len() != 36 || !session.bytes().all(|c| c.is_ascii_hexdigit() || c == b'-') {
        return Err("RUN_NATIVE_ID stage=native_binding".into());
    }
    let cwd = Path::new(
        input["cwd"]
            .as_str()
            .ok_or("RUN_NATIVE_CWD stage=native_binding")?,
    );
    if !cwd.starts_with(project) || cwd.components().any(|c| matches!(c, Component::ParentDir)) {
        return Err("RUN_NATIVE_SCOPE stage=native_binding".into());
    }
    let transcript = PathBuf::from(
        input["transcript_path"]
            .as_str()
            .ok_or("RUN_NATIVE_TRANSCRIPT stage=native_binding")?,
    );
    crate::claude_transcript::validate(home, session, &transcript).map_err(|e| e.to_string())?;
    Ok(Some(Confirmation {
        instance: instance.into(),
        session: session.into(),
        source: input["source"].as_str().unwrap().into(),
        transcript_bytes: std::fs::metadata(&transcript)
            .map(|m| m.len())
            .unwrap_or_default(),
        transcript,
        name: input["session_title"].as_str().map(str::to_owned),
    }))
}

pub fn confirmed(native: &kah_run::native::Native) -> bool {
    native.confirmation.as_ref().is_some_and(|c| {
        native.instance.as_deref() == Some(&c.instance)
            && native.session.as_deref() == Some(&c.session)
            && std::fs::metadata(&c.transcript).is_ok_and(|m| m.is_file() && m.len() > 0)
    })
}
