//! Preserve host-native hook events when another controller job holds the store.
use super::{hook, state::Store};
use crate::Result;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::fs;
#[derive(Serialize, Deserialize)]
pub struct Event {
    pub input: Value,
    pub pid: Option<u32>,
    pub pane: Option<String>,
    pub socket: Option<std::path::PathBuf>,
    #[serde(default)]
    pub context: Option<super::request_context::Context>,
    /// Set only after receive validates the host event's original binding.
    #[serde(default)]
    pub validated_pane: Option<String>,
}
impl Event {
    pub fn capture(input: Value, pid: Option<u32>) -> Self {
        Self {
            input,
            pid,
            pane: std::env::var("HERDR_PANE_ID").ok(),
            socket: std::env::var_os("HERDR_SOCKET_PATH").map(Into::into),
            context: None,
            validated_pane: None,
        }
    }
    pub(super) fn retired_for(&self, state: &super::state::State) -> bool {
        self.validated_pane.is_some()
            && self.validated_pane == self.pane
            && self.pane != state.observer.pane
            && self.socket == state.worker.socket
            && state
                .observer
                .session
                .as_deref()
                .is_some_and(|session| self.input["session_id"].as_str() == Some(session))
            && self.input["cwd"].as_str().is_some_and(|cwd| {
                super::control::contains_workspace(&state.observer.workspace, cwd)
            })
    }
}
pub fn enqueue(worker: &str, event: &Event) -> Result<()> {
    let root = Store::directory(worker)?.join("hook-events");
    kah_core::run_store::ensure_private_directory(&root)?;
    crate::unattended_report::save(
        &root,
        &format!("{}.json", crate::relay_store::nonce()),
        &serde_json::to_value(event)?,
    )?;
    hook::spawn("__review-hook-events", worker)
}
pub fn run(worker: &str) -> Result<()> {
    let root = Store::directory(worker)?.join("hook-events");
    if !root.exists() {
        return Ok(());
    }
    loop {
        let files = entries(&root)?;
        if files.is_empty() {
            return Ok(());
        }
        for path in files {
            let event: Event =
                super::state::read(&path)?.ok_or("REVIEW_HOOK_EVENT_MISSING stage=hook")?;
            hook::apply_wait(worker, &event)?;
            fs::remove_file(path)?;
        }
    }
}
fn entries(root: &std::path::Path) -> Result<Vec<std::path::PathBuf>> {
    let mut files = fs::read_dir(root)?
        .map(|e| e.map(|e| e.path()))
        .collect::<std::io::Result<Vec<_>>>()?;
    files.sort();
    files.retain(|p| p.extension().is_some_and(|e| e == "json"));
    Ok(files)
}
