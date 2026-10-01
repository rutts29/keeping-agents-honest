//! Parent-bound commands and cancellation of an owned non-terminal process group.
use std::{
    os::unix::process::CommandExt,
    process::{Child, Command, ExitStatus},
    time::Duration,
};

pub fn parent_bound(native: &Command, signal: &str) -> Command {
    let mut command = Command::new("/usr/bin/setpriv");
    command
        .args([
            "--pdeathsig",
            signal,
            "/bin/sh",
            "-c",
            "test \"$PPID\" -eq \"$1\" || exit 125; shift; exec \"$@\"",
            "kah-owned-child",
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
    command
}

pub fn wait(
    command: &mut Command,
    mut interrupted: impl FnMut() -> Result<bool, String>,
) -> Result<Option<ExitStatus>, String> {
    let mut child = Owned {
        child: command.process_group(0).spawn().map_err(failure)?,
        reaped: false,
    };
    loop {
        if let Some(status) = child.child.try_wait().map_err(failure)? {
            child.reaped = true;
            return Ok(Some(status));
        }
        if interrupted()? {
            return Ok(None);
        }
        std::thread::sleep(Duration::from_millis(100));
    }
}
struct Owned {
    child: Child,
    reaped: bool,
}
impl Drop for Owned {
    fn drop(&mut self) {
        if self.reaped {
            return;
        }
        let group = format!("-{}", self.child.id());
        // Keep the leader unreaped until both signals are sent, preventing PID
        // reuse while terminating its owned descendants.
        let _ = Command::new("/bin/kill")
            .args(["-TERM", "--", &group])
            .output();
        std::thread::sleep(Duration::from_millis(250));
        let _ = Command::new("/bin/kill")
            .args(["-KILL", "--", &group])
            .output();
        let _ = self.child.wait();
    }
}
fn failure(error: std::io::Error) -> String {
    format!("RUN_CHILD_IO stage=process: {error}")
}
