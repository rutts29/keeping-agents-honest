//! Native Rust child supervision retains the actual code/signal, including a
//! child which cannot log its own failure. There is no execution deadline.
use super::outcome;
use crate::Result;
use serde_json::json;
use std::{
    os::unix::process::ExitStatusExt,
    path::Path,
    process::{Command, ExitStatus, Stdio},
};
pub fn reason(status: ExitStatus) -> String {
    if let Some(signal) = status.signal() {
        format!(
            "REVIEW_CONSUME_SIGNAL stage=supervision: signal {signal}; OOM attribution is not inferred"
        )
    } else if status.success() {
        "REVIEW_CONSUME_EXIT_OK stage=supervision".into()
    } else {
        format!(
            "REVIEW_CONSUME_EXIT_ERROR stage=supervision: exit {}",
            status.code().unwrap_or(-1)
        )
    }
}
pub fn supervise(worker: &str) -> Result<()> {
    let home =
        std::path::PathBuf::from(std::env::var_os("HOME").ok_or("REVIEW_HOME stage=supervision")?);
    let installed = home.join(".local/bin/kah");
    let program = if installed.is_file() {
        installed
    } else {
        let current = std::env::current_exe()?;
        if current.file_name().is_none_or(|name| name != "kah") || !current.is_file() {
            return Err("REVIEW_PROGRAM_MISSING stage=supervision".into());
        }
        current
    };
    execute(worker, &program)
}
pub fn execute(worker: &str, program: &Path) -> Result<()> {
    let result = execute_inner(worker, program);
    if let Err(error) = &result {
        outcome::record(
            worker,
            json!({"kind":"supervisor_failure","reason":error.to_string()}),
        )?;
    }
    result
}
fn execute_inner(worker: &str, program: &Path) -> Result<()> {
    let started = std::time::Instant::now();
    let started_ms = kah_observer::session::now_ms();
    let request = outcome::pending(worker)?;
    let mut child = Command::new(program)
        .args(["__review-consume-worker", worker])
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .spawn()
        .map_err(|e| format!("REVIEW_CONSUME_SPAWN stage=supervision: {e}"))?;
    outcome::record(
        worker,
        json!({"kind":"child_started","child_pid":child.id()}),
    )?;
    let status = child
        .wait()
        .map_err(|e| format!("REVIEW_CONSUME_WAIT stage=supervision: {e}"))?;
    let reported = super::supervision_failure::reported(worker, child.id(), started_ms, &request)?;
    let reason = reported
        .as_ref()
        .and_then(|v| v["reason"].as_str())
        .map(str::to_owned)
        .unwrap_or_else(|| reason(status));
    outcome::record(
        worker,
        json!({"kind":"child_exited","request":request,"child_pid":child.id(),"exit_code":status.code(),"signal":status.signal(),"reason":reason,"typed_return":reported,"elapsed_ms":started.elapsed().as_millis()}),
    )?;
    eprintln!("{reason}");
    if status.success() {
        Ok(())
    } else {
        Err(super::supervision_failure::Failure {
            request,
            child_pid: child.id(),
            reason,
        }
        .into())
    }
}
