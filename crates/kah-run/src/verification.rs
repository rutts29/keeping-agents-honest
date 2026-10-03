//! Tests use the run's executor policy, never a host project shell.
use kah_sandbox::Executor;
use std::{process::Command, time::Duration};

pub fn command(executor: &Executor, script: &str, timeout: Duration) -> Result<Command, String> {
    if script.trim().is_empty()
        || script.len() > 64 * 1024
        || script.contains('\0')
        || timeout.is_zero()
    {
        return Err("RUN_TEST_ARGUMENTS stage=verification".into());
    }
    let tail = [
        "/bin/sh",
        "-c",
        "umask 0002; exec \"$@\"",
        "kah-executor",
        "/opt/codex",
        "exec-server",
        "--listen",
        "stdio",
    ];
    if executor.args.len() < 9 || executor.args[executor.args.len() - 8..] != tail {
        return Err("RUN_EXECUTOR_SHAPE stage=verification".into());
    }
    let mut command = Command::new("/usr/bin/timeout");
    command.stdin(std::process::Stdio::null());
    command
        .args([
            "--signal=TERM",
            "--kill-after=2s",
            &format!("{}s", timeout.as_secs_f64()),
            "/usr/bin/docker",
        ])
        .args(&executor.args[..executor.args.len() - 8])
        // Keep a guest deadline even if the host controller/docker client dies.
        .args([
            "/usr/bin/timeout",
            "--signal=TERM",
            "--kill-after=2s",
            &format!("{}s", timeout.as_secs_f64()),
        ])
        .args(["/bin/sh", "-lc", &format!("umask 0002; {script}")]);
    Ok(command)
}

#[derive(serde::Serialize, serde::Deserialize)]
pub struct Receipt {
    pub commit: String,
    pub command: String,
    pub success: bool,
    pub exit_code: Option<i32>,
    pub elapsed_ms: u64,
    pub output: String,
    pub output_truncated: bool,
}

pub fn execute(
    executor: &Executor,
    script: &str,
    commit: &str,
    timeout: Duration,
    log: &std::path::Path,
) -> Result<Receipt, String> {
    execute_checked(executor, script, commit, timeout, log, || Ok(false))
}
pub fn execute_checked(
    executor: &Executor,
    script: &str,
    commit: &str,
    timeout: Duration,
    log: &std::path::Path,
    interrupted: impl FnMut() -> Result<bool, String>,
) -> Result<Receipt, String> {
    use std::{fs::OpenOptions, io::Read, os::unix::fs::OpenOptionsExt, time::Instant};
    if !matches!(commit.len(), 40 | 64) || !commit.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err("RUN_TEST_COMMIT stage=verification".into());
    }
    let output = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(log)
        .map_err(|e| format!("RUN_TEST_LOG stage=verification: {e}"))?;
    let error = output
        .try_clone()
        .map_err(|e| format!("RUN_TEST_LOG stage=verification: {e}"))?;
    let started = Instant::now();
    let mut command = crate::child::parent_bound(&command(executor, script, timeout)?, "TERM");
    command
        .stdin(std::process::Stdio::null())
        .stdout(output)
        .stderr(error);
    let status = crate::child::wait(&mut command, interrupted)?;
    let mut bytes = Vec::new();
    std::fs::File::open(log)
        .and_then(|mut file| file.read_to_end(&mut bytes))
        .map_err(|e| format!("RUN_TEST_READ stage=verification: {e}"))?;
    Ok(Receipt {
        commit: commit.into(),
        command: script.into(),
        success: status.is_some_and(|s| s.success()),
        exit_code: status.and_then(|s| s.code()),
        elapsed_ms: started.elapsed().as_millis() as u64,
        output: format!(
            "{}{}",
            if status.is_none() {
                "RUN_TEST_INTERRUPTED stage=verification\n"
            } else {
                ""
            },
            String::from_utf8_lossy(&bytes)
        ),
        output_truncated: false,
    })
}
