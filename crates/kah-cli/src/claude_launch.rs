//! Native Claude launch; directory policy is invocation-local.
use crate::{Arguments, Result};
use kah_observer::{compaction::CompactorConfig, judge::routing::Backend, session::Binding};
use kah_sandbox::{Domain, Project};
use serde_json::json;
use std::{path::Path, process::Command};

pub fn prepare(home: &Path, cwd: &Path, args: Arguments) -> Result<Command> {
    let explicit = args.project.as_ref().map(|(d, n)| (*d, n.as_str()));
    let project = Project::select(&home.join("repos"), cwd, explicit)?;
    prepare_project(home, project, args)
}

pub fn prepare_project(home: &Path, project: Project, args: Arguments) -> Result<Command> {
    args.options.validate(project.domain)?;
    validate(&args, project.domain != Domain::Mine)?;
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
    configure(home, &project, &args, &mut command)?;
    if let Some(effort) = args.effort {
        command.args(["--effort", &effort]);
    }
    if args.options.yolo {
        command.args(["--permission-mode", "bypassPermissions"]);
    }
    command.args(args.native);
    Ok(command)
}

fn configure(
    home: &Path,
    project: &Project,
    args: &Arguments,
    command: &mut Command,
) -> Result<()> {
    if args.no_observer && project.domain == Domain::Mine {
        command.env_remove("KAH_CLAUDE_BINDING");
        return Ok(());
    }
    let mut settings = if project.domain != Domain::Mine {
        let setup = crate::claude_setup::prepare(home, project, args)?;
        setup.apply(command);
        setup.settings
    } else {
        json!({"hooks":{}})
    };
    if !args.no_observer {
        let binding = binding(home, &project.root, args)?;
        let handler = home.join(".local/bin/kah");
        settings["disableAllHooks"] = false.into();
        for event in [
            "SessionStart",
            "UserPromptSubmit",
            "PreToolUse",
            "PostToolUse",
            "PostToolUseFailure",
            "Stop",
        ] {
            let groups = settings["hooks"]
                .as_object_mut()
                .unwrap()
                .entry(event)
                .or_insert_with(|| json!([]));
            groups.as_array_mut().ok_or("CLAUDE_HOOKS_TYPE stage=launch")?.push(json!({"hooks":[{"type":"command","command":handler,"args":["__claude-hook"],"timeout":120}]}));
        }
        command.env("KAH_CLAUDE_BINDING", serde_json::to_string(&binding)?);
    } else {
        command.env_remove("KAH_CLAUDE_BINDING");
        let groups = settings["hooks"]
            .as_object_mut()
            .unwrap()
            .entry("SessionStart")
            .or_insert_with(|| json!([]));
        groups.as_array_mut().ok_or("CLAUDE_HOOKS_TYPE stage=launch")?.push(json!({"hooks":[{"type":"command","command":home.join(".local/bin/kah"),"args":["__claude-session"],"timeout":2}]}));
    }
    command.args(["--settings", &settings.to_string()]);
    Ok(())
}

fn binding(home: &Path, project: &Path, args: &Arguments) -> Result<Binding> {
    let config = CompactorConfig::requested(
        args.observer_model
            .as_deref()
            .unwrap_or("claude-sonnet-5-5"),
        Some(args.observer_effort.as_deref().unwrap_or("low")),
    )?;
    if !config.model().starts_with("claude-") {
        return Err(
            "OBSERVER_PROVIDER_MODEL stage=observer_config: Claude observer needs a Claude model"
                .into(),
        );
    }
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

fn validate(args: &Arguments, restricted: bool) -> Result<()> {
    if (!args.no_observer || restricted)
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
            return Err("CLAUDE_UNATTENDED_BOUNDARY stage=arguments: kah user workflows require a full native session".into());
        }
        if (!args.no_observer || restricted)
            && matches!(
                flag,
                "--bare" | "--safe-mode" | "--settings" | "--setting-sources"
            )
        {
            return Err("CLAUDE_OBSERVER_POLICY stage=arguments: this option can disable or replace kah hooks".into());
        }
        if restricted
            && (flag == "--dangerously-skip-permissions"
                || flag == "--allow-dangerously-skip-permissions"
                || arg == "--permission-mode=bypassPermissions")
        {
            return Err("CLAUDE_PERMISSION_POLICY stage=arguments: restricted native file permissions must remain active".into());
        }
    }
    if restricted
        && args
            .native
            .windows(2)
            .any(|p| p[0] == "--permission-mode" && p[1] == "bypassPermissions")
    {
        return Err("CLAUDE_PERMISSION_POLICY stage=arguments: restricted native file permissions must remain active".into());
    }
    Ok(())
}
