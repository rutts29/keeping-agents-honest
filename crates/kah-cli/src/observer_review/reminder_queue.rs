//! One durable slot per finding; later review kinds cannot replace its delivery.
use crate::Result;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    fs,
    path::{Path, PathBuf},
};

pub fn save(root: &Path, value: &Value) -> Result<()> {
    let _delivery = super::state::Store::delivery_at(root, true)?;
    save_locked(root, value)
}
fn save_locked(root: &Path, value: &Value) -> Result<()> {
    let directory = root.join("reminders");
    kah_core::run_store::ensure_private_directory(&directory)?;
    let identity = if value["request"].is_string() {
        serde_json::to_vec(&(&value["request"], &value["purpose"]))?
    } else {
        serde_json::to_vec(value)?
    };
    let name = format!("{:x}.json", Sha256::digest(identity));
    let mut value = value.clone();
    let order = super::state::read::<Value>(&directory.join(&name))?
        .map(|v| v["delivery_order"].clone())
        .unwrap_or_else(|| {
            value
                .get("delivery_order")
                .cloned()
                .unwrap_or_else(|| crate::relay_store::nonce().into())
        });
    value["delivery_order"] = order;
    crate::unattended_report::save(&directory, &name, &value)
}
pub fn next(root: &Path) -> Result<Option<(PathBuf, Value)>> {
    Ok(pending(root)?.into_iter().next())
}
pub fn pending(root: &Path) -> Result<Vec<(PathBuf, Value)>> {
    let _delivery = super::state::Store::delivery_at(root, true)?;
    pending_locked(root)
}
pub(crate) fn next_locked(store: &super::state::Store) -> Result<Option<(PathBuf, Value)>> {
    Ok(pending_locked(&store.directory)?.into_iter().next())
}
fn pending_locked(root: &Path) -> Result<Vec<(PathBuf, Value)>> {
    let legacy = root.join("reminder.json");
    if let Some(mut value) = super::state::read::<Value>(&legacy)? {
        let at = fs::metadata(&legacy)?
            .modified()?
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        value["delivery_order"] = format!("{at:032x}-legacy").into();
        save_locked(root, &value)?;
        fs::remove_file(&legacy)?;
    }
    let directory = root.join("reminders");
    if !directory.exists() {
        return Ok(vec![]);
    }
    let mut entries = Vec::new();
    for entry in fs::read_dir(directory)? {
        let path = entry?.path();
        if path.extension().is_some_and(|s| s == "json") {
            let value = super::state::read::<Value>(&path)?
                .ok_or("REVIEW_REMINDER_MISSING stage=delivery")?;
            entries.push((path, value));
        }
    }
    entries.sort_by(|a, b| {
        a.1["delivery_order"]
            .as_str()
            .cmp(&b.1["delivery_order"].as_str())
    });
    Ok(entries)
}
