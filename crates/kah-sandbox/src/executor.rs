use crate::{Error, LaunchOptions, Project};
use std::path::Path;

// Keep the operator's default ACL writable when gVisor creates files as a
// different host uid. This changes creation permissions, never ownership.
const ENTRYPOINT: &[&str] = &["/bin/sh", "-c", "umask 0002; exec \"$@\"", "kah-executor"];

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

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Executor {
    pub args: Vec<String>,
    image_index: usize,
}

impl Executor {
    /// Operator-owned host files appear as nobody inside the rootless sandbox.
    /// Private worktrees lack the domain directories' inherited service group.
    pub fn use_nobody(&mut self) {
        for arg in &mut self.args {
            if arg == "0:0" {
                *arg = "65534:65534".into();
            } else if arg == "/home/sandbox:rw,nosuid,nodev,size=512m,mode=700" {
                arg.push_str(",uid=65534,gid=65534");
            }
        }
    }
    pub fn set_network(&mut self, online: bool) -> Result<(), Error> {
        let at = self.args[..self.image_index]
            .iter()
            .position(|arg| arg == "--network")
            .ok_or_else(|| Error::new("EXECUTOR_NETWORK", "executor", "network option absent"))?;
        self.args[at + 1] = if online { "bridge" } else { "none" }.into();
        Ok(())
    }
    /// Reuse containment and mount policy for a native command hook or shell
    /// prefix. The native command stays argv, never part of the entrypoint text.
    pub fn command(&self, cwd: &Path, command: &[String]) -> Result<std::process::Command, Error> {
        if self.image_index >= self.args.len() || command.is_empty() {
            return Err(Error::new(
                "EXECUTOR_COMMAND",
                "executor",
                "invalid command specification",
            ));
        }
        let mut args = self.args[..=self.image_index].to_vec();
        let workdir = args
            .windows(2)
            .position(|pair| pair[0] == "--workdir")
            .ok_or_else(|| Error::new("EXECUTOR_WORKDIR", "executor", "workdir is absent"))?;
        args[workdir + 1] = mount_path(cwd)?.into();
        let mut process = std::process::Command::new("/usr/bin/docker");
        process.args(args).args(ENTRYPOINT).args(command);
        Ok(process)
    }

    pub fn add_writable_mount(&mut self, source: &Path, target: &Path) -> Result<(), Error> {
        let mount = format!(
            "type=bind,source={},target={}",
            mount_path(source)?,
            mount_path(target)?
        );
        if !self.args[..self.image_index].contains(&mount) {
            self.insert_options(["--mount".into(), mount]);
        }
        Ok(())
    }
    pub fn with_read_only_mounts(
        mut self,
        mounts: &[(std::path::PathBuf, std::path::PathBuf)],
    ) -> Result<Self, Error> {
        for (source, target) in mounts {
            // A read-only parent must not retain a writable nested bind, and
            // promoting an existing bind must not create duplicate targets.
            for index in 0..self.image_index.saturating_sub(1) {
                if self.args[index] != "--mount" {
                    continue;
                }
                let arg = &mut self.args[index + 1];
                if let Some(path) = arg.split(',').find_map(|part| part.strip_prefix("target="))
                    && Path::new(path).starts_with(target)
                    && !arg.ends_with(",readonly")
                {
                    arg.push_str(",readonly");
                }
            }
            let mount = format!(
                "type=bind,source={},target={},readonly",
                mount_path(source)?,
                mount_path(target)?
            );
            if !self.args[..self.image_index].contains(&mount) {
                self.insert_options(["--mount".into(), mount]);
            }
        }
        Ok(self)
    }

    /// Docker options stay before the image even when an adapter wraps the
    /// container command. Paths still pass through the mount validator above.
    pub fn insert_options(&mut self, options: impl IntoIterator<Item = String>) {
        let options = options.into_iter().collect::<Vec<_>>();
        let count = options.len();
        self.args
            .splice(self.image_index..self.image_index, options);
        self.image_index += count;
    }

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
        let image_index = args.len() + 2; // The daemon selector is prepended below.
        args.push(image.into());
        args.extend(ENTRYPOINT.iter().map(|arg| (*arg).into()));
        args.extend([
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
        Ok(Self { args, image_index })
    }
}

/// runsc could not start the sandbox (seen under host CPU pressure). docker
/// fails at container create, before anything runs inside it.
pub fn sandbox_start_failed(stderr: &str) -> bool {
    stderr.contains("OCI runtime create failed")
        && stderr.contains(
            "cannot create sandbox: cannot read client sync file: waiting for sandbox to start: EOF",
        )
}

/// Runs a gVisor command to completion. Only that start failure is retried,
/// once, after a short pause: the container never started, so nothing ran
/// twice. Containers are --rm and unnamed, so a retry cannot collide. Any
/// other failure, or a second start failure, is returned unchanged.
pub fn output_retrying_start(
    command: &mut std::process::Command,
) -> std::io::Result<std::process::Output> {
    let first = command.output()?;
    if first.status.success() || !sandbox_start_failed(&String::from_utf8_lossy(&first.stderr)) {
        return Ok(first);
    }
    std::thread::sleep(std::time::Duration::from_secs(2));
    command.output()
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
