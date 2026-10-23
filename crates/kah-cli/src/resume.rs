//! Resolve native saved-session location before choosing a domain overlay.
use crate::{
    Arguments, Result,
    native_options::{Take, takes},
};
use kah_codex::{
    control::Control,
    overlay::{self, Policy},
};
use kah_sandbox::Project;
use serde_json::json;
use std::{
    path::{Path, PathBuf},
    time::Duration,
};

pub(crate) fn target(home: &Path, cwd: &Path, args: &mut Arguments) -> Result<PathBuf> {
    let Some(id) = session_id(&args.native).map(str::to_owned) else {
        return Ok(cwd.into());
    };
    let inspection = overlay_root(home).join("control");
    overlay::prepare(
        &home.join(".codex"),
        &inspection,
        &Policy {
            restricted: true,
            online: false,
            root: cwd,
            cwd,
        },
    )?;
    let mut client = Control::connect(
        &home.join(".local/bin/codex"),
        &inspection,
        cwd,
        Duration::from_secs(10),
    )?;
    let response = client.request("thread/read", json!({"threadId":id,"includeTurns":false}))?;
    let saved = PathBuf::from(
        response["thread"]["cwd"]
            .as_str()
            .ok_or("RESUME_CWD stage=resume: native session has no cwd")?,
    );
    let rollout = PathBuf::from(
        response["thread"]["path"]
            .as_str()
            .ok_or("RESUME_PATH stage=resume: native session has no stored path")?,
    );
    let saved = restore_policy(&saved, &rollout, home, &id, args)?;
    let explicit = args.project.as_ref().map(|(d, n)| (*d, n.as_str()));
    let selected = Project::select(&home.join("repos"), &saved, explicit)?;
    if !saved.starts_with(&selected.root) {
        return Err(
            "RESUME_PROJECT_MISMATCH stage=resume: explicit project differs from the saved session"
                .into(),
        );
    }
    Ok(saved)
}

fn restore_policy(
    cwd: &Path,
    rollout: &Path,
    home: &Path,
    id: &str,
    args: &mut Arguments,
) -> Result<PathBuf> {
    let base = kah_core::run_store::RunStore::default_state_root()?;
    if let Some(policy) = crate::session_policy::saved(&base, id)? {
        args.options.online = matches!(
            policy.policy.as_str(),
            "research-online" | "untrusted-online"
        );
        args.options.yolo = policy.policy == "mine-yolo";
        // Native resume can persist its new cwd even when our hook rejects the
        // turn. The immutable kah binding remains the routing authority.
        return Ok(if cwd.starts_with(&policy.project) {
            cwd.into()
        } else {
            policy.project
        });
    }
    apply_location(home, rollout, args);
    Ok(cwd.into())
}

/// The Codex overlays' parent in the kah state root.
pub fn overlay_root(home: &Path) -> PathBuf {
    kah_core::run_store::RunStore::state_root_for(home).join("codex")
}

/// Only a native session's kah-owned rollout path carries a previous opt-in.
pub fn apply_location(home: &Path, rollout: &Path, args: &mut Arguments) {
    let Ok(relative) = rollout.strip_prefix(overlay_root(home)) else {
        return;
    };
    match relative
        .components()
        .next()
        .and_then(|c| c.as_os_str().to_str())
    {
        Some("research-online" | "untrusted-online") => args.options.online = true,
        Some("mine-yolo") => args.options.yolo = true,
        _ => (),
    }
}

/// The saved session a `resume` or `fork` names. Only the first argument that
/// is neither an option nor an option's value can be that subcommand, and
/// nothing after the native `--` is an option.
pub fn session_id(args: &[String]) -> Option<&str> {
    let mut tokens = args.iter();
    while let Some(token) = tokens.next() {
        match token.as_str() {
            "--" => return None,
            "resume" | "fork" => {
                let id = tokens.next()?;
                return (id.len() == 36 && id.bytes().all(|b| b.is_ascii_hexdigit() || b == b'-'))
                    .then_some(id.as_str());
            }
            token if takes(token) == Take::Next => {
                tokens.next();
            }
            token if token.starts_with('-') => (),
            _ => return None,
        }
    }
    None
}
