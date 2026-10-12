//! Native provider refusals are terminal input/turn outcomes, never report fixes.
use super::state::{Control, Store};
use crate::Result;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::path::PathBuf;
mod cursor;
mod reader;
mod screen;
pub use screen::active as precaution;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct NativeBlock {
    pub source: PathBuf,
    pub offset: u64,
    pub end: u64,
    pub sha256: String,
    pub code: String,
    pub timestamp: Option<String>,
    pub record: String,
}
pub fn reason(error: &dyn std::fmt::Display) -> bool {
    error.to_string().split_whitespace().next() == Some("REVIEW_PROVIDER_BLOCKED")
}
pub fn recorded(worker: &str) -> Result<Option<Value>> {
    let Some(state) = super::state::read_control(worker)? else {
        return Ok(None);
    };
    let value: Option<Value> =
        super::state::read(&Store::directory(worker)?.join("provider-block.json"))?;
    Ok(value.filter(|v| {
        v["status"] == "provider_blocked"
            && v["observer"].as_str() == state.observer.session.as_deref()
    }))
}
pub fn require_active(worker: &str) -> Result<()> {
    if recorded(worker)?.is_some() {
        return Err(
            "REVIEW_PROVIDER_BLOCKED stage=native_provider: human decision required; no delivery"
                .into(),
        );
    }
    Ok(())
}
pub fn detect(provider: &str, path: &std::path::Path) -> Result<Option<NativeBlock>> {
    reader::detect(provider, path)
}
pub fn observe(worker: &str) -> Result<()> {
    if recorded(worker)?.is_some() {
        return require_active(worker);
    }
    let Some(state) = super::state::read_control(worker)? else {
        return Ok(());
    };
    if let Some(native) = cursor::observe(&state)? {
        retain(worker, Some(native), "native terminal policy error")?;
        return require_active(worker);
    }
    Ok(())
}
pub fn retain(worker: &str, native: Option<NativeBlock>, evidence: &str) -> Result<()> {
    if recorded(worker)?.is_some() {
        return Ok(());
    }
    let Some(state) = super::state::read_control(worker)? else {
        return Ok(());
    };
    let root = Store::directory(worker)?;
    let _lock = Store::delivery_at(&root.join("provider-block"), true)?;
    if recorded(&state.worker.session)?.is_some() {
        return Ok(());
    }
    let request = state.pending.as_ref().map(|p| p.id.as_str());
    let text = format!(
        "[kah-observer-provider-blocked] Worker {} review {}: native provider blocked this observer. Pending evidence is retained; no automatic redelivery, keys or replacement. Human/provider decision required.",
        state.worker.session,
        request.unwrap_or("queued")
    );
    let record = json!({"at_ms":kah_observer::session::now_ms(),"request":request,
        "observer":state.observer.session,"status":"provider_blocked","origin":"host",
        "error":"REVIEW_PROVIDER_BLOCKED stage=native_provider: human decision required",
        "native":native,"evidence":evidence,"text":text,"automatic_redelivery":false});
    super::source_store::save(&root, "provider-block.json", &record)?;
    super::source_store::save(&root, "gap.json", &record)?;
    pause(&root)?;
    reviewer(&state, &text, &root)
}
fn pause(root: &std::path::Path) -> Result<()> {
    use std::{fs::OpenOptions, io::Write, os::unix::fs::OpenOptionsExt};
    let marker = root
        .parent()
        .ok_or("REVIEW_PROVIDER_BINDING stage=native_provider")?
        .join("observer-paused");
    match OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(marker)
    {
        Ok(mut file) => {
            writeln!(
                file,
                "Native provider blocked; wait for the user's decision."
            )?;
            file.sync_all()?;
        }
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => (),
        Err(e) => return Err(e.into()),
    }
    Ok(())
}
fn reviewer(state: &Control, text: &str, root: &std::path::Path) -> Result<()> {
    let sent = state.worker.socket.as_deref().map(|socket| {
        crate::run_pane::invoke_at(
            &[
                "agent",
                "prompt",
                "kah-reviewer",
                &format!("[kah-builder] {text}"),
            ],
            Some(socket),
        )
    });
    super::source_store::save(
        root,
        "provider-block-notice.json",
        &json!({"observer":state.observer.session,
        "submitted":sent.as_ref().is_some_and(|r|r.is_ok()),"error":sent.and_then(|r|r.err()).map(|e|e.to_string())}),
    )
}
pub fn stops_watch(error: &dyn std::fmt::Display) -> bool {
    reason(error) || error.to_string().starts_with("REVIEW_PAUSED")
}
pub fn take_notice(worker: &str) -> Result<Option<Value>> {
    let Some(mut block) = recorded(worker)? else {
        return Ok(None);
    };
    let root = Store::directory(worker)?.join("provider-block");
    let _lock = Store::delivery_at(&root, true)?;
    let identity =
        json!({"observer":block["observer"],"native":block["native"],"error":block["error"]});
    let prior: Option<Value> = super::state::read(&root.join("hook-notice.json"))?;
    if prior.as_ref() == Some(&identity) {
        return Ok(None);
    }
    if !block["text"].is_string() {
        block["text"]=format!("[kah-observer-provider-blocked] Worker {worker} review {}: native provider blocked the observer; pending retained, no keys/redelivery/replacement. Human/provider decision required.",block["request"].as_str().unwrap_or("queued")).into();
    }
    super::source_store::save(&root, "hook-notice.json", &identity)?;
    Ok(Some(block))
}
