//! A pane owned by one unattended run, addressed only by the returned Herdr ID.
use crate::Result;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    path::Path,
    process::Command,
    thread,
    time::{Duration, Instant},
};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Pane {
    pub id: String,
}
impl Pane {
    pub fn start(command: &Command) -> Result<Self> {
        if std::env::var("HERDR_ENV").as_deref() != Ok("1") {
            return Err("RUN_HERDR_REQUIRED stage=launch: start kah run inside Herdr".into());
        }
        let cwd = command
            .get_current_dir()
            .ok_or("RUN_CWD_REQUIRED stage=launch")?;
        let pane = Self::create(cwd)?;
        let start = (|| -> Result<()> {
            pane.wait_shell()?;
            invoke(&[
                "pane",
                "run",
                &pane.id,
                &crate::native_run::shell_command(command)?,
            ])?;
            Ok(())
        })();
        if let Err(error) = start {
            let _ = pane.close();
            return Err(error);
        }
        Ok(pane)
    }
    fn create(cwd: &Path) -> Result<Self> {
        let response = call(&[
            "pane",
            "split",
            "--current",
            "--direction",
            "down",
            "--cwd",
            cwd.to_str().ok_or("RUN_CWD_UTF8 stage=launch")?,
            "--no-focus",
        ])?;
        let id = response["result"]["pane"]["pane_id"]
            .as_str()
            .ok_or("RUN_PANE_ID stage=launch")?
            .to_owned();
        Ok(Self { id })
    }
    fn wait_shell(&self) -> Result<()> {
        let deadline = Instant::now() + Duration::from_secs(15);
        loop {
            let pane = self.inspect()?;
            if pane["agent"].is_null() && pane["foreground_cwd"].is_string() {
                return Ok(());
            }
            if Instant::now() >= deadline {
                return Err("RUN_PANE_START_TIMEOUT stage=launch".into());
            }
            thread::sleep(Duration::from_millis(100));
        }
    }
    pub fn inspect(&self) -> Result<Value> {
        Ok(call(&["pane", "get", &self.id])?["result"]["pane"].clone())
    }
    pub fn prompt(&self, text: &str) -> Result<()> {
        invoke(&["agent", "prompt", &self.id, text])?;
        Ok(())
    }
    pub fn notify_waiting(&self, run: &str, reason: &str) -> Result<()> {
        invoke(&[
            "notification",
            "show",
            "kah run needs attention",
            "--body",
            &format!("{run}: {reason}. Open pane {}. kah status {run}", self.id),
            "--sound",
            "request",
        ])?;
        Ok(())
    }
    pub fn logs(&self, session: Option<&str>) -> Result<String> {
        let pane = self.inspect()?;
        if session.is_some_and(|id| {
            pane["agent_session"]["value"]
                .as_str()
                .is_some_and(|actual| actual != id)
        }) {
            return Err("RUN_PANE_REPLACED stage=run_logs".into());
        }
        let output = invoke(&[
            "pane", "read", &self.id, "--source", "recent", "--format", "text",
        ])?;
        Ok(String::from_utf8_lossy(&output.stdout).into_owned())
    }
    pub fn close(&self) -> Result<()> {
        invoke(&["pane", "close", &self.id])?;
        Ok(())
    }
}
/// Herdr owns dialog detection. Unknown startup is reported without guessing
/// at terminal text or submitting keys to native consent/permission dialogs.
pub fn waiting_reason(pane: &Value, submitted: bool, seconds: u64) -> Option<&'static str> {
    if pane["agent_status"] == "blocked" {
        Some("native agent is waiting for consent, permission or input")
    } else if !submitted && seconds >= 15 {
        Some("startup has not submitted the task; inspect native consent or startup")
    } else {
        None
    }
}
fn call(arguments: &[&str]) -> Result<Value> {
    let output = invoke(arguments)?;
    serde_json::from_slice(&output.stdout).map_err(|_| {
        format!(
            "RUN_HERDR_JSON stage=pane: {} {} returned non-JSON output",
            arguments[0], arguments[1]
        )
        .into()
    })
}
fn invoke(arguments: &[&str]) -> Result<std::process::Output> {
    let output = Command::new("herdr")
        .args(arguments)
        .output()
        .map_err(|e| format!("RUN_HERDR_IO stage=pane: {e}"))?;
    if !output.status.success() {
        return Err(format!(
            "RUN_HERDR_COMMAND stage=pane: {} {}: {}",
            arguments[0],
            arguments[1],
            String::from_utf8_lossy(&output.stderr)
                .chars()
                .take(1024)
                .collect::<String>()
        )
        .into());
    }
    Ok(output)
}
