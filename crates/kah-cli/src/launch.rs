use crate::{Arguments, Result};
use kah_codex::overlay::{self, Policy};
use kah_sandbox::{Domain, Executor, Project, current_uid, publish_binary};
use sha2::{Digest, Sha256};
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

pub struct Launch {
    pub project: Project,
    pub overlay: PathBuf,
    pub environment_id: Option<String>,
    binary: PathBuf,
    overrides: Vec<String>,
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
        args.options.validate(project.domain)?;
        args.validate_native(project.domain)?;
        if project.needs_creation {
            project.initialize()?;
        }
        let cwd = project.root.join(&project.relative_cwd);
        let restricted = project.domain != Domain::Mine;
        let (policy_name, overlay) = overlay_directory(home, &project, &args);
        overlay::prepare(
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
            Some(prepare_executor(
                home,
                &project,
                &args,
                &binary,
                &overlay,
                &policy_name,
            )?)
        } else {
            None
        };
        let mut overrides = policy_overrides(&project, &cwd, &args);
        let session_policy = serde_json::to_string(&crate::session_policy::SessionPolicy {
            project: project.root.clone(),
            policy: policy_name,
            overlay: overlay.clone(),
        })?;
        let observer_binding = configure_observation(home, &binary, &overlay, &project, &args)?;
        overrides.push("features.hooks=true".into());
        // Prevent the native resume picker from loading another project's host
        // config before SessionStart can compare the saved kah policy.
        overrides.push("tui.resume_cwd=\"current\"".into());
        Ok(Self {
            project,
            overlay,
            environment_id,
            binary,
            overrides,
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
            for pair in self.native.windows(2) {
                if pair[0] == "--model" || pair[0] == "-m" {
                    command.args([
                        "-c",
                        &format!("model={}", serde_json::to_string(&pair[1]).unwrap()),
                    ]);
                } else if pair[0] == "-c" || pair[0] == "--config" {
                    command.args(["-c", &pair[1]]);
                }
            }
        }
        for setting in &self.overrides {
            command.args(["-c", setting]);
        }
        if !app_server {
            command.args(&self.native);
        }
        command
    }
}

fn policy_overrides(project: &Project, cwd: &Path, args: &Arguments) -> Vec<String> {
    if project.domain == Domain::Mine {
        if args.options.yolo {
            return vec![
                "default_permissions=\":danger-full-access\"".into(),
                "approval_policy=\"never\"".into(),
            ];
        }
        return vec![
            "default_permissions=\"kah-mine-workspace\"".into(),
            "permissions.kah-mine-workspace.extends=\":workspace\"".into(),
            "permissions.kah-mine-workspace.network.enabled=true".into(),
            "approval_policy=\"on-request\"".into(),
            "approvals_reviewer=\"auto_review\"".into(),
        ];
    }
    let mut result = vec![
        "default_permissions=\":danger-full-access\"".into(),
        "approval_policy=\"never\"".into(),
        format!(
            "web_search=\"{}\"",
            if args.options.online {
                "live"
            } else {
                "disabled"
            }
        ),
    ];
    for path in [&project.root, &cwd.to_path_buf()] {
        result.push(format!(
            "projects.{}.trust_level=\"untrusted\"",
            serde_json::to_string(path).expect("path encoding validated")
        ));
    }
    result
}

fn service_uid() -> Result<u32> {
    let output = Command::new("/usr/bin/id")
        .args(["-u", "sbxrun"])
        .output()
        .map_err(|e| format!("SANDBOX_ACCOUNT stage=launch: {e}"))?;
    Ok(String::from_utf8_lossy(&output.stdout)
        .trim()
        .parse()
        .map_err(|e| format!("SANDBOX_ACCOUNT stage=launch: {e}"))?)
}

fn prepare_executor(
    home: &Path,
    project: &Project,
    args: &Arguments,
    binary: &Path,
    overlay: &Path,
    policy_name: &str,
) -> Result<String> {
    let public = publish_binary(
        binary,
        &PathBuf::from(format!("/var/tmp/kah-runtime-{}", current_uid()?)),
    )?;
    let image = fs::read_to_string("/usr/local/share/sbx/image-ref")
        .map_err(|e| format!("SANDBOX_IMAGE_READ stage=launch: {e}"))?;
    let mounts = kah_sandbox::publish_skills(
        home,
        &PathBuf::from(format!("/var/tmp/kah-skills-{}", current_uid()?)),
    )?;
    let executor = Executor::new(project, args.options, &public, image.trim(), service_uid()?)?
        .with_read_only_mounts(&mounts)?;
    let id = format!("kah-{policy_name}");
    overlay::set_executor(overlay, &id, &executor.args)?;
    Ok(id)
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
    let overlay = home
        .join(".local/state/kah/codex")
        .join(&policy_name)
        .join(&hash[..16]);
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
    let model = args.observer_model.as_deref().unwrap_or("gpt-6-luna");
    let effort = args.observer_effort.as_deref().unwrap_or("low");
    let config = kah_observer::semantic_resolver::CompactorConfig::new(model, Some(effort))?;
    let binding = kah_observer::session::Binding {
        session: String::new(),
        project: project.root.clone(),
        codex_home: home.join(".codex"),
        native: native.into(),
        model: config.model().into(),
        effort: config.reasoning().into(),
        timeout_secs: config.timeout().as_secs(),
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
        .filter(|h| !args.no_observer || h.event != "Stop")
        .collect::<Vec<_>>();
    overlay::install_owned_hooks(native, directory, &project.root, &hooks)?;
    if args.no_observer {
        return Ok(None);
    }
    Ok(Some(serde_json::to_string(&binding)?))
}
