//! The pane's controller owns one native child; kernel parent-death bounds it.
use crate::Result;
use std::{
    path::Path,
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};

pub struct NativeChild {
    child: Child,
    terminal: String,
}
impl NativeChild {
    pub fn spawn(native: &Command) -> Result<Self> {
        let saved = Command::new("stty")
            .arg("-g")
            .stdin(Stdio::inherit())
            .output()?;
        if !saved.status.success() {
            return Err("RUN_TTY_REQUIRED stage=run_launch".into());
        }
        let terminal = String::from_utf8(saved.stdout)?.trim().to_owned();
        let mut command = Command::new("/usr/bin/setpriv");
        command
            .args([
                "--pdeathsig",
                "KILL",
                "/bin/sh",
                "-c",
                "test \"$PPID\" -eq \"$1\" || exit 125; shift; exec \"$@\"",
                "kah-native-session",
            ])
            .arg(std::process::id().to_string())
            .arg(native.get_program())
            .args(native.get_args());
        if let Some(cwd) = native.get_current_dir() {
            command.current_dir(cwd);
        }
        for (name, value) in native.get_envs() {
            if let Some(value) = value {
                command.env(name, value);
            } else {
                command.env_remove(name);
            }
        }
        let child = command
            .spawn()
            .map_err(|e| format!("RUN_NATIVE_SPAWN stage=run_launch: {e}"))?;
        Ok(Self { child, terminal })
    }
    pub fn exited(&mut self) -> Result<bool> {
        Ok(self.child.try_wait()?.is_some())
    }
    pub fn stop(&mut self) -> Result<()> {
        if self.child.try_wait()?.is_none() {
            let _ = Command::new("/bin/kill")
                .args(["-TERM", &self.child.id().to_string()])
                .output();
            let deadline = Instant::now() + Duration::from_secs(2);
            while self.child.try_wait()?.is_none() && Instant::now() < deadline {
                std::thread::sleep(Duration::from_millis(20));
            }
            if self.child.try_wait()?.is_none() {
                self.child.kill()?;
            }
        }
        self.child.wait()?;
        if !Command::new("stty")
            .arg(&self.terminal)
            .stdin(Stdio::inherit())
            .output()?
            .status
            .success()
        {
            return Err("RUN_TTY_RESTORE stage=run_cleanup".into());
        }
        Ok(())
    }
}
impl Drop for NativeChild {
    fn drop(&mut self) {
        let _ = self.stop();
    }
}

pub fn stop_executors(project: &Path) -> Result<()> {
    let socket = format!(
        "unix:///run/user/{}/docker.sock",
        kah_run::runtime::service_uid()?
    );
    let output = Command::new("/usr/bin/docker")
        .args(["--host", &socket, "ps", "-aq", "--filter"])
        .arg(format!("label=dev.kah.project={}", project.display()))
        .output()?;
    if !output.status.success() {
        return Err("RUN_CONTAINER_LIST stage=run_cleanup".into());
    }
    for id in String::from_utf8_lossy(&output.stdout).split_whitespace() {
        if !id.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err("RUN_CONTAINER_ID stage=run_cleanup".into());
        }
        let removed = Command::new("/usr/bin/docker")
            .args(["--host", &socket, "rm", "-f", id])
            .output()?;
        if !removed.status.success() {
            // --rm may finish between listing and removal. Confirm absence;
            // a daemon failure or a still-present container is a real error.
            let listed = Command::new("/usr/bin/docker")
                .args([
                    "--host",
                    &socket,
                    "ps",
                    "-aq",
                    "--filter",
                    &format!("id={id}"),
                ])
                .output()?;
            if !listed.status.success() || !listed.stdout.is_empty() {
                return Err("RUN_CONTAINER_REMOVE stage=run_cleanup".into());
            }
        }
    }
    Ok(())
}
