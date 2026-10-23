use crate::{Arguments, Result};
use kah_codex::overlay::{self, Policy};
use kah_sandbox::{Domain, Project};
use sha2::{Digest, Sha256};
use std::{
    path::{Path, PathBuf},
    process::Command,
};

pub struct Launch {
    pub project: Project,
    pub overlay: PathBuf,
    pub environment_id: Option<String>,
    binary: PathBuf,
    native: Vec<String>,
    observer_binding: Option<String>,
    session_policy: String,
}

impl Launch {
    pub fn prepare(home: &Path, cwd: &Path, mut args: Arguments) -> Result<Self> {
        let target = crate::resume::target(home, cwd, &mut args)?;
        let explicit = args
            .project
            .as_ref()
            .map(|(domain, name)| (*domain, name.as_str()));
        let project = Project::select(&home.join("repos"), &target, explicit)?;
        Self::prepare_project(home, project, args)
    }

    /// Worktree runs supply their host-owned project binding directly.
    pub fn prepare_project(home: &Path, project: Project, args: Arguments) -> Result<Self> {
        args.options.validate(project.domain)?;
        args.validate_native(project.domain)?;
        if project.needs_creation {
            project.initialize()?;
        } else if project.ensure_identity()? {
            eprintln!("kah set git user.name/email for this project from your global config");
        }
        let cwd = project.root.join(&project.relative_cwd);
        let restricted = project.domain != Domain::Mine;
        let (policy_name, overlay) = overlay_directory(home, &project, &args);
        overlay::prepare_native(
            &home.join(".codex"),
            &overlay,
            &Policy {
                restricted,
                online: args.options.online,
                root: &project.root,
                cwd: &cwd,
            },
        )?;
        let binary = home
            .join(".local/bin/codex")
            .canonicalize()
            .map_err(|e| format!("CODEX_BINARY stage=launch: {e}"))?;
        let environment_id = if restricted {
            Some(crate::native_setup::codex(
                home,
                &project,
                &overlay,
                &policy_name,
                args.options,
                args.worktree_git.as_deref(),
            )?)
        } else {
            None
        };
        overlay::write_settings(&overlay, policy_settings(&project, &args))?;
        let session_policy = serde_json::to_string(&crate::session_policy::SessionPolicy {
            project: project.root.clone(),
            policy: policy_name,
            overlay: overlay.clone(),
        })?;
        let observer_binding = configure_observation(home, &binary, &overlay, &project, &args)?;
        Ok(Self {
            project,
            overlay,
            environment_id,
            binary,
            native: args.native,
            observer_binding,
            session_policy,
        })
    }

    /// The same launch policy drives native TUI and app-server acceptance tests.
    pub fn command(&self, app_server: bool) -> Command {
        let mut command = Command::new(&self.binary);
        command
            .env("CODEX_HOME", &self.overlay)
            .env("KAH_SESSION_POLICY", &self.session_policy)
            .env_remove("CODEX_EXEC_SERVER_URL")
            .current_dir(self.project.root.join(&self.project.relative_cwd));
        if let Some(binding) = &self.observer_binding {
            command.env("KAH_OBSERVER_BINDING", binding);
        } else {
            command.env_remove("KAH_OBSERVER_BINDING");
        }
        if app_server {
            command.args(["app-server", "--stdio"]);
        }
        if !app_server {
            if self.environment_id.is_some() {
                command
                    .arg("--cd")
                    .arg(self.project.root.join(&self.project.relative_cwd));
            }
            if !self.native.iter().any(|arg| arg == "--no-daemon") {
                command.arg("--no-daemon");
            }
            command.args(&self.native);
        }
        command
    }
}

fn policy_settings(project: &Project, args: &Arguments) -> serde_json::Value {
    use serde_json::json;
    let mut settings = json!({"features":{"hooks":true},"tui":{"resume_cwd":"current"}});
    if project.domain == Domain::Mine && !args.options.yolo {
        settings["default_permissions"] = "kah-mine-workspace".into();
        settings["permissions"] =
            json!({"kah-mine-workspace":{"extends":":workspace","network":{"enabled":true}}});
        settings["approval_policy"] = "on-request".into();
        settings["approvals_reviewer"] = "auto_review".into();
    } else {
        settings["default_permissions"] = ":danger-full-access".into();
        settings["approval_policy"] = "never".into();
    }
    if let Some(effort) = &args.effort {
        settings["model_reasoning_effort"] = effort.clone().into();
    }
    // app-server has no --model flag; the explicit choice also configures its home.
    if let Some(model) = args.native_model() {
        settings["model"] = model.into();
    }
    settings
}

fn overlay_directory(home: &Path, project: &Project, args: &Arguments) -> (String, PathBuf) {
    let policy_name = format!(
        "{}-{}",
        project.domain.as_str(),
        if args.options.yolo {
            "yolo"
        } else if args.options.online {
            "online"
        } else {
            "offline"
        }
    );
    let hash = format!(
        "{:x}",
        Sha256::digest(project.root.as_os_str().as_encoded_bytes())
    );
    let overlay = crate::resume::overlay_for(
        home,
        &policy_name,
        &hash[..16],
        args.saved_overlay.as_deref(),
    );
    (policy_name, overlay)
}

fn configure_observation(
    home: &Path,
    native: &Path,
    directory: &Path,
    project: &Project,
    args: &Arguments,
) -> Result<Option<String>> {
    let handler = home.join(".local/bin/kah");
    let model = args.observer_model.as_deref().unwrap_or("gpt-6.1-sol");
    let effort = args.observer_effort.as_deref().unwrap_or("low");
    let config = kah_observer::compaction::CompactorConfig::requested(model, Some(effort))?;
    if !config.model().starts_with("gpt-") {
        return Err(
            "OBSERVER_PROVIDER_MODEL stage=observer_config: Codex observer needs a Codex model"
                .into(),
        );
    }
    let binding = kah_observer::session::Binding {
        session: String::new(),
        project: project.root.clone(),
        codex_home: home.join(".codex"),
        native: native.into(),
        model: config.model().into(),
        effort: config.reasoning().into(),
        timeout_secs: config.timeout().as_secs(),
        jev_model: args.jev_model.as_deref().unwrap_or("jev-latest").into(),
        judge: kah_observer::judge::routing::Backend::for_domain(
            project.domain.as_str(),
            args.observer_judge.as_deref(),
        )?,
    };
    let command = format!(
        "'{}' __hook",
        handler.to_string_lossy().replace('\'', "'\\''")
    );
    let hooks = [
        ("SessionStart", "sessionStart", 120),
        ("UserPromptSubmit", "userPromptSubmit", 2),
        ("Stop", "stop", 120),
        // Mine/Research route this event only to commit/tag/relay metadata.
        // The handler retains completed-tool Jev checks only for Untrusted.
        ("PostToolUse", "postToolUse", 120),
        ("PreToolUse", "preToolUse", 120),
    ]
    .into_iter()
    .map(|(event, api_event, timeout)| overlay::OwnedHook {
        event,
        api_event,
        timeout,
        command: command.clone(),
    })
    .collect::<Vec<_>>();
    let hooks = hooks
        .into_iter()
        .filter(|h| {
            !args.no_observer
                || matches!(h.event, "SessionStart" | "UserPromptSubmit" | "PreToolUse")
        })
        .collect::<Vec<_>>();
    overlay::install_owned_hooks(native, directory, &project.root, &hooks)?;
    if args.no_observer {
        return Ok(None);
    }
    Ok(Some(serde_json::to_string(&binding)?))
}

#[cfg(test)]
mod tests;
