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
    let tail = ["/opt/codex", "exec-server", "--listen", "stdio"];
    if executor.args.len() < 5 || executor.args[executor.args.len() - 4..] != tail {
        return Err("RUN_EXECUTOR_SHAPE stage=verification".into());
    }
    let mut command = Command::new("/usr/bin/timeout");
    command
        .args([
            "--signal=TERM",
            "--kill-after=2s",
            &format!("{}s", timeout.as_secs_f64()),
            "/usr/bin/docker",
        ])
        .args(&executor.args[..executor.args.len() - 4])
        .args(["/bin/sh", "-lc", script]);
    Ok(command)
}
