//! Mine's native Claude launcher. Restricted domains fail before native startup.
use crate::{Arguments, Result};
use kah_observer::{compaction::CompactorConfig, judge::routing::Backend, session::Binding};
use kah_sandbox::{Domain, Project};
use serde_json::json;
use std::{path::Path, process::Command};

pub fn prepare(home: &Path, cwd: &Path, args: Arguments) -> Result<Command> {
    let explicit = args.project.as_ref().map(|(d, n)| (*d, n.as_str()));
    let project = Project::select(&home.join("repos"), cwd, explicit)?;
    if project.domain != Domain::Mine {
        return Err("CLAUDE_BOUNDARY_UNAVAILABLE stage=launch: native file tools have no proven gVisor adapter; see docs/claude-adapter.md".into());
    }
    args.options.validate(project.domain)?;
    validate(&args)?;
    if project.needs_creation {
        project.initialize()?;
    }
    let binary = home.join(".local/bin/claude");
    binary
        .canonicalize()
        .map_err(|e| format!("CLAUDE_BINARY stage=launch: {e}"))?;
    // Preserve stock argv[0]/comm through the installed symlink. Versioned
    // target names (for example 2.1.286) are not recognizable agent names.
    let mut command = Command::new(binary);
    command
        .current_dir(project.root.join(&project.relative_cwd))
        .env_remove("CODEX_THREAD_ID")
        .env_remove("KAH_OBSERVER_BINDING")
        .env_remove("KAH_SESSION_POLICY");
    if !args.no_observer {
        let binding = binding(home, &project.root, &args)?;
        let handler = home.join(".local/bin/kah");
        let hook = format!(
            "'{}' __claude-hook",
            handler.to_string_lossy().replace('\'', "'\\''")
        );
        let mut settings = json!({"disableAllHooks":false,"hooks":{}});
        for event in ["SessionStart", "UserPromptSubmit", "PreToolUse", "Stop"] {
            settings["hooks"][event] =
                json!([{"hooks":[{"type":"command","command":hook,"timeout":120}]}]);
        }
        command
            .args(["--settings", &settings.to_string()])
            .env("KAH_CLAUDE_BINDING", serde_json::to_string(&binding)?);
    } else {
        command.env_remove("KAH_CLAUDE_BINDING");
    }
    if let Some(effort) = args.effort {
        command.args(["--effort", &effort]);
    }
    if args.options.yolo {
        command.args(["--permission-mode", "bypassPermissions"]);
    }
    command.args(args.native);
    Ok(command)
}

fn binding(home: &Path, project: &Path, args: &Arguments) -> Result<Binding> {
    let config = CompactorConfig::new(
        args.observer_model.as_deref().unwrap_or("gpt-6-luna"),
        Some(args.observer_effort.as_deref().unwrap_or("low")),
    )?;
    Ok(Binding {
        session: String::new(),
        project: project.into(),
        codex_home: home.join(".codex"),
        native: home.join(".local/bin/codex"),
        model: config.model().into(),
        effort: config.reasoning().into(),
        timeout_secs: 45,
        judge: Backend::for_domain("mine", args.observer_judge.as_deref())?,
        jev_model: args
            .jev_model
            .clone()
            .unwrap_or_else(|| "jev-latest".into()),
    })
}

fn validate(args: &Arguments) -> Result<()> {
    if !args.no_observer
        && ["CLAUDE_CODE_SAFE_MODE", "CLAUDE_CODE_SIMPLE"]
            .iter()
            .any(|name| {
                std::env::var(name)
                    .is_ok_and(|v| matches!(v.to_ascii_lowercase().as_str(), "1" | "true" | "yes"))
            })
    {
        return Err("CLAUDE_OBSERVER_POLICY stage=arguments: inherited safe/bare mode disables kah hooks; unset it or explicitly use --no-observer".into());
    }
    for arg in &args.native {
        let flag = arg.split('=').next().unwrap_or(arg);
        if matches!(flag, "-p" | "--print" | "--bg" | "--background") {
            return Err("CLAUDE_UNATTENDED_BOUNDARY stage=arguments: unattended Claude requires a proven gVisor inference boundary".into());
        }
        if !args.no_observer
            && matches!(
                flag,
                "--bare" | "--safe-mode" | "--settings" | "--setting-sources"
            )
        {
            return Err("CLAUDE_OBSERVER_POLICY stage=arguments: this option can disable or replace kah hooks".into());
        }
    }
    Ok(())
}
