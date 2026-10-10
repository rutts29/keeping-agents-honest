//! Context selection belongs to its review, including a yielded gate host job.
use super::super::state::{self, Store};
use crate::Result;
use serde_json::{Value, json};

pub fn activate(worker: &str, review: &str, decision: &str) -> Result<()> {
    validate(review)?;
    validate(decision)?;
    let root = Store::directory(worker)?;
    let cursors = root.join("preparation-cursors");
    kah_core::run_store::ensure_private_directory(&cursors)?;
    let value = json!({"review":review,"decision":decision});
    super::super::source_store::save(&cursors, &format!("{review}.json"), &value)?;
    let owners = cursors.join("owners");
    kah_core::run_store::ensure_private_directory(&owners)?;
    super::super::source_store::save(
        &owners,
        &format!("{decision}.json"),
        &json!({"review":review}),
    )?;
    if state::read_control(worker)?.is_none_or(|s| s.pending.is_none_or(|p| p.id == review)) {
        crate::unattended_report::save(&root, "preparation-current.json", &value)?;
    }
    Ok(())
}

pub fn active(worker: &str, review: &str) -> Result<String> {
    validate(review)?;
    let root = Store::directory(worker)?;
    let scoped: Option<Value> = state::read(
        &root
            .join("preparation-cursors")
            .join(format!("{review}.json")),
    )?;
    let legacy: Option<Value> = if scoped.is_none() {
        state::read(&root.join("preparation-current.json"))?
    } else {
        None
    };
    Ok(scoped
        .or(legacy)
        .filter(|v| v["review"] == review)
        .and_then(|v| v["decision"].as_str().map(str::to_owned))
        .unwrap_or_else(|| review.into()))
}

pub fn owner(worker: &str, decision: &str) -> Result<Option<String>> {
    validate(decision)?;
    let root = Store::directory(worker)?;
    let owned: Option<Value> = state::read(
        &root
            .join("preparation-cursors/owners")
            .join(format!("{decision}.json")),
    )?;
    Ok(owned.and_then(|v| v["review"].as_str().map(str::to_owned)))
}
fn validate(id: &str) -> Result<()> {
    if !id.starts_with("review-") || !id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-') {
        return Err("PREPARATION_CURSOR_ID stage=context_preparation".into());
    }
    Ok(())
}

pub fn review(request: &super::super::state::Request) -> Result<String> {
    if let Some(parent) = request.trigger["parent_review"].as_str() {
        return Ok(parent.into());
    }
    if let Some(parent) = super::super::gate_host::parent(&request.worker, &request.id)? {
        return Ok(parent);
    }
    Ok(Store::read(&request.worker)?
        .and_then(|s| s.pending.map(|p| p.id))
        .unwrap_or_else(|| request.id.clone()))
}
