use crate::{Error, LaunchOptions, Project};
use std::path::Path;

const BASE_ARGS: &[&str] = &[
    "run",
    "--rm",
    "-i",
    "--runtime",
    "runsc",
    "--read-only",
    "--user",
    "0:0",
    "--cap-drop",
    "ALL",
    "--security-opt",
    "no-new-privileges=true",
    "--init",
    "--restart",
    "no",
    "--ulimit",
    "core=0:0",
    "--ulimit",
    "nofile=4096:8192",
    "--tmpfs",
    "/tmp:rw,nosuid,nodev,size=512m,mode=1777",
    "--tmpfs",
    "/run:rw,nosuid,nodev,size=64m,mode=755",
    "--tmpfs",
    "/home/sandbox:rw,nosuid,nodev,size=512m,mode=700",
    "--env",
    "HOME=/home/sandbox",
    "--env",
    "GIT_CONFIG_COUNT=1",
    "--env",
    "GIT_CONFIG_KEY_0=safe.directory",
];

#[derive(Debug)]
pub struct Executor {
    pub args: Vec<String>,
}

impl Executor {
    pub fn read_only_project(mut self) -> Self {
        for index in 0..self.args.len().saturating_sub(1) {
            if self.args[index] == "--mount" && !self.args[index + 1].ends_with(",readonly") {
                self.args[index + 1].push_str(",readonly");
            }
        }
        self
    }

    pub fn new(
        project: &Project,
        options: LaunchOptions,
        binary: &Path,
        image: &str,
        uid: u32,
    ) -> Result<Self, Error> {
        options.validate(project.domain)?;
        let root = mount_path(&project.root)?;
        let binary = mount_path(binary)?;
        if image.is_empty() || image.starts_with('-') || image.contains(char::is_whitespace) {
            return Err(Error::new(
                "SANDBOX_IMAGE",
                "executor",
                "invalid installed image reference",
            ));
        }
        let mut args: Vec<String> = BASE_ARGS.iter().map(|arg| (*arg).into()).collect();
        for (key, value) in [
            (
                "--network",
                if options.online {
                    "bridge".into()
                } else {
                    "none".into()
                },
            ),
            (
                "--workdir",
                project
                    .root
                    .join(&project.relative_cwd)
                    .to_string_lossy()
                    .into_owned(),
            ),
            ("--mount", format!("type=bind,source={root},target={root}")),
            (
                "--mount",
                format!("type=bind,source={binary},target=/opt/codex,readonly"),
            ),
            ("--env", format!("GIT_CONFIG_VALUE_0={root}")),
            ("--label", format!("dev.kah.project={root}")),
            (
                "--label",
                format!("dev.kah.domain={}", project.domain.as_str()),
            ),
        ] {
            args.extend([key.into(), value]);
        }
        args.extend([
            image.into(),
            "/opt/codex".into(),
            "exec-server".into(),
            "--listen".into(),
            "stdio".into(),
        ]);
        args.splice(
            0..0,
            [
                "--host".into(),
                format!("unix:///run/user/{uid}/docker.sock"),
            ],
        );
        Ok(Self { args })
    }
}

fn mount_path(path: &Path) -> Result<&str, Error> {
    let text = path
        .to_str()
        .filter(|p| path.is_absolute() && !p.contains([',', '\n', '\r']))
        .ok_or_else(|| {
            Error::new(
                "MOUNT_PATH",
                "executor",
                "mount paths must be absolute UTF-8 without commas or newlines",
            )
        })?;
    Ok(text)
}
