//! Full native sessions on the controller's private worktree.
use crate::{Arguments, Launch, Result};
use kah_run::worktree::Worktree;
use kah_sandbox::{Domain, Project};
use std::{path::Path, process::Command};

pub fn command(
    home: &Path,
    tree: &Worktree,
    mut args: Arguments,
    task: &str,
    resume: Option<&str>,
) -> Result<Command> {
    args.worktree_git = Some(tree.git_dir.clone());
    // Even a Mine unattended run may change only its own worktree. The source
    // domain remains in the run record; execution uses the restricted launcher.
    let project = Project {
        domain: Domain::Research,
        root: tree.path.clone(),
        relative_cwd: Default::default(),
        needs_creation: false,
    };
    if let Some(session) = resume {
        args.native.extend([
            if args.agent.as_deref() == Some("claude") {
                "--resume"
            } else {
                "resume"
            }
            .into(),
            session.into(),
        ]);
    } else {
        args.native.extend(["--".into(), task.into()]);
    }
    match args.agent.as_deref() {
        Some("claude") => crate::claude_launch::prepare_project(home, project, args),
        None | Some("codex") => Ok(Launch::prepare_project(home, project, args)?.command(false)),
        _ => Err("RUN_AGENT stage=launch: choose codex or claude".into()),
    }
}

/// Herdr runs the already prepared native argv in a shell. Quote each component
/// independently; task text and JSON bindings are data, never shell source.
pub fn shell_command(command: &Command) -> Result<String> {
    let mut parts = vec!["env".to_owned()];
    for (key, _) in command.get_envs().filter(|(_, value)| value.is_none()) {
        parts.extend(["-u".into(), quote(key)?]);
    }
    for (key, value) in command.get_envs().filter_map(|(k, v)| v.map(|v| (k, v))) {
        let mut pair = key.to_os_string();
        pair.push("=");
        pair.push(value);
        parts.push(quote(&pair)?);
    }
    parts.push(quote(command.get_program())?);
    for arg in command.get_args() {
        parts.push(quote(arg)?);
    }
    Ok(parts.join(" "))
}
fn quote(text: &std::ffi::OsStr) -> Result<String> {
    let text = text.to_str().ok_or("RUN_COMMAND_UTF8 stage=launch")?;
    Ok(format!("'{}'", text.replace('\'', "'\\''")))
}
