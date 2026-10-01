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
    pub fn close(&self) -> Result<()> {
        invoke(&["pane", "close", &self.id])?;
        Ok(())
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
