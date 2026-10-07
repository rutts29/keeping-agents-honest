use super::state::State;
use crate::{Arguments, Launch, Result, sandbox_command::Spec};
use kah_codex::overlay::{self, OwnedHook};
use kah_sandbox::{Domain, Project};
use serde_json::{Value, json};
use std::{path::Path, process::Command};

pub fn command(home: &Path, state: &State, prompt: &str) -> Result<Command> {
    let args = arguments(state, prompt)?;
    let project = Project {
        domain: Domain::Research,
        root: state.observer.workspace.clone(),
        relative_cwd: Default::default(),
        needs_creation: false,
    };
    let mut command = if state.worker.provider == "claude" {
        claude(home, state, project, args)?
    } else {
        codex(home, state, project, args)?
    };
    for name in [
        "CODEX_THREAD_ID",
        "KAH_OBSERVER_BINDING",
        "KAH_CLAUDE_BINDING",
        "KAH_RUN_ID",
        "KAH_RUN_INSTANCE",
    ] {
        command.env_remove(name);
    }
    Ok(command)
}

fn arguments(state: &State, prompt: &str) -> Result<Arguments> {
    let choice = kah_observer::compaction::CompactorConfig::new(
        &state.observer.model,
        Some(&state.observer.effort),
    )?;
    let words = vec![
        "--no-observer".into(),
        "--model".into(),
        choice.model().into(),
        "--effort".into(),
        choice.reasoning().into(),
    ];
    let mut args = Arguments::parse(words)?;
    if state.worker.provider == "claude" {
        args.agent = Some("claude".into());
        args.native.extend([
            "--permission-mode".into(),
            "acceptEdits".into(),
            "--allowedTools".into(),
            "Bash".into(),
        ]);
    }
    if state.worker.provider == "codex" {
        for setting in run_config(state)? {
            args.native.extend(["-c".into(), setting]);
        }
    }
    if let Some(session) = &state.observer.session {
        args.native.extend([
            if state.worker.provider == "claude" {
                "--resume"
            } else {
                "resume"
            }
            .into(),
            session.clone(),
        ]);
    } else if state.worker.provider == "claude" {
        args.native
            .extend(["--name".into(), state.observer.name.clone()]);
    }
    if !prompt.is_empty() {
        args.native.extend(["--".into(), prompt.into()]);
    }
    Ok(args)
}

#[cfg(test)]
#[path = "../../tests/observer_unit/launch_flags.rs"]
mod tests;

fn codex(home: &Path, state: &State, project: Project, args: Arguments) -> Result<Command> {
    let launch = Launch::prepare_project(
        home,
        Project {
            domain: project.domain,
            root: project.root.clone(),
            relative_cwd: project.relative_cwd.clone(),
            needs_creation: false,
        },
        args,
    )?;
    let mut executor = crate::native_setup::executor(home, &project, Default::default(), None)?;
    executor.use_nobody();
    executor = executor.with_read_only_mounts(&[(project.root.clone(), project.root.clone())])?;
    executor.add_writable_mount(&project.root.join("outbox"), &project.root.join("outbox"))?;
    let mut online = executor.clone();
    online.set_network(true)?;
    let id = launch
        .environment_id
        .as_ref()
        .ok_or("REVIEW_EXECUTOR stage=launch")?;
    overlay::set_executors(
        &launch.overlay,
        id,
        &[
            (id, &executor.args),
            ("kah-mcp-online", &online.args),
            ("kah-mcp-offline", &executor.args),
        ],
    )?;
    let path = launch.overlay.join("control/hooks.json");
    if path.exists() {
        let mut spec = Spec::load(&path)?;
        spec.executor = executor;
        spec.save(&path)?;
    }
    // This is a kah-generated, read-only data workspace, never the worker repo.
    overlay::write_settings(
        &launch.overlay,
        json!({"projects":{project.root.to_string_lossy().as_ref():{"trust_level":"trusted"}}}),
    )?;
    codex_hooks(home, state, &launch)?;
    Ok(launch.command(false))
}
fn codex_hooks(home: &Path, state: &State, launch: &Launch) -> Result<()> {
    let handler = home.join(".local/bin/kah");
    let command = format!(
        "{} __review-hook {}",
        quote(&handler.to_string_lossy()),
        quote(&state.worker.session)
    );
    let hooks = [
        ("SessionStart", "sessionStart"),
        ("PostToolUse", "postToolUse"),
        ("Stop", "stop"),
        ("SessionEnd", "sessionEnd"),
    ]
    .map(|(event, api_event)| OwnedHook {
        event,
        api_event,
        command: command.clone(),
        timeout: 2,
    });
    overlay::install_owned_hooks(
        &home.join(".local/bin/codex"),
        &launch.overlay,
        &launch.project.root,
        &hooks,
    )?;
    Ok(())
}

fn claude(home: &Path, state: &State, project: Project, args: Arguments) -> Result<Command> {
    let original = crate::claude_launch::prepare_project(
        home,
        Project {
            domain: project.domain,
            root: project.root.clone(),
            relative_cwd: project.relative_cwd.clone(),
            needs_creation: false,
        },
        args,
    )?;
    let mut arguments = original
        .get_args()
        .map(|s| s.to_os_string())
        .collect::<Vec<_>>();
    let index = arguments
        .iter()
        .position(|s| s == "--settings")
        .ok_or("REVIEW_SETTINGS stage=launch")?
        + 1;
    let mut settings: Value = serde_json::from_str(
        arguments[index]
            .to_str()
            .ok_or("REVIEW_SETTINGS_UTF8 stage=launch")?,
    )?;
    for event in ["SessionStart", "PostToolUse", "Stop", "SessionEnd"] {
        let groups = settings["hooks"]
            .as_object_mut()
            .unwrap()
            .entry(event)
            .or_insert_with(|| json!([]));
        groups.as_array_mut().unwrap().push(json!({"hooks":[{"type":"command","command":home.join(".local/bin/kah"),"args":["__review-hook",state.worker.session],"timeout":2}]}));
    }
    for name in ["inputs", "bin", "AGENTS.md", "CLAUDE.md"] {
        let path = project.root.join(name);
        settings["permissions"]["deny"]
            .as_array_mut()
            .unwrap()
            .extend([
                json!(format!("Edit(/{})", path.display())),
                json!(format!("Edit(/{}/**)", path.display())),
            ]);
    }
    let scratch = claude_scratch(&project.root, &mut settings)?;
    arguments[index] = settings.to_string().into();
    let private = crate::claude_setup::runtime_directory(&project)?.join("offline/control");
    for name in ["executor", "mcp-online", "mcp-offline"] {
        let path = private.join(format!("{name}.json"));
        let mut spec = Spec::load(&path)?;
        spec.executor = spec
            .executor
            .with_read_only_mounts(&[(project.root.clone(), project.root.clone())])?;
        spec.executor
            .add_writable_mount(&project.root.join("outbox"), &project.root.join("outbox"))?;
        spec.save(&path)?;
    }
    let mut command = Command::new(original.get_program());
    command.args(arguments).current_dir(&project.root);
    for (key, value) in original.get_envs() {
        if let Some(value) = value {
            command.env(key, value);
        } else {
            command.env_remove(key);
        }
    }
    command.env("CLAUDE_CODE_TMPDIR", scratch);
    Ok(command)
}
fn quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

fn run_config(state: &State) -> Result<Vec<String>> {
    let Some(id) = state
        .worker
        .project
        .parent()
        .and_then(|p| p.file_name())
        .and_then(|name| name.to_str())
        .filter(|name| name.starts_with("run-"))
    else {
        return Ok(vec![]);
    };
    let root = kah_core::run_store::RunStore::default_state_root()?.join("runs");
    let record = kah_run::store::Store::read(&root, id)?;
    if record.worktree != state.worker.project {
        return Err("REVIEW_RUN_SCOPE stage=launch".into());
    }
    Ok(record
        .native
        .map(|n| n.codex_config)
        .unwrap_or_default()
        .into_iter()
        .filter(|setting| {
            setting
                .split_once('=')
                .is_some_and(|(key, _)| key == "service_tier")
        })
        .collect())
}

fn claude_scratch(root: &Path, settings: &mut Value) -> Result<std::path::PathBuf> {
    // anthropics/claude-code v2.1.292/CHANGELOG.md (2.1.5): native internal-temp
    // override. The runtime dir is intentionally denied to host file tools;
    // observer scratch belongs under its existing writable outbox mount.
    let scratch = root.join("outbox/tmp");
    std::fs::create_dir_all(&scratch)?;
    settings["env"]["CLAUDE_CODE_TMPDIR"] = json!(scratch);
    Ok(scratch)
}
