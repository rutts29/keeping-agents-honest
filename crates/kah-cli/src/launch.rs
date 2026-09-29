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
                &project,
                &args,
                &binary,
                &overlay,
                &policy_name,
            )?)
        } else {
            None
        };
        let overrides = policy_overrides(&project, &cwd, &args);
        if restricted && std::env::var("HERDR_ENV").as_deref() == Ok("1") {
            let handler = home.join(".local/bin/kah");
            overlay::install_session_hook(&binary, &overlay, &cwd, &handler)?;
        }
        Ok(Self {
            project,
            overlay,
            environment_id,
            binary,
            overrides,
            native: args.native,
        })
    }

    /// The same launch policy drives native TUI and app-server acceptance tests.
    pub fn command(&self, app_server: bool) -> Command {
        let mut command = Command::new(&self.binary);
        command
            .env("CODEX_HOME", &self.overlay)
            .env_remove("CODEX_EXEC_SERVER_URL")
            .current_dir(self.project.root.join(&self.project.relative_cwd));
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
    let executor = Executor::new(project, args.options, &public, image.trim(), service_uid()?)?;
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
