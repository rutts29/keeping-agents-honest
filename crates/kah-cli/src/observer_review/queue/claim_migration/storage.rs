//! Publish classification before retiring the exact digest-checked original.
use super::*;
use sha2::{Digest, Sha256};
use std::path::Path;
pub(super) fn finish(root: &Path, path: &Path) -> Result<()> {
    let status = crate::observer_review::source_store::read_selected(
        path,
        &crate::observer_review::source_store::Select::fields(&[(
            "state",
            crate::observer_review::source_store::Select::All,
        )]),
    )?;
    if status["state"] == "complete" {
        return Ok(());
    }
    let mut record: Value = crate::observer_review::source_store::read(path)?;
    let old = record["original"]
        .as_str()
        .ok_or("REVIEW_CLAIM_MIGRATION_RECORD stage=queue_migration")?;
    let source = root.join("queued").join(old);
    let archive = root.join("queue-retired");
    kah_core::run_store::ensure_private_directory(&archive)?;
    let retired = archive.join(old);
    let present = if source.exists() { &source } else { &retired };
    if record["digest"] != format!("{:x}", Sha256::digest(fs::read(present)?)) {
        return Err("REVIEW_CLAIM_MIGRATION_SOURCE_CHANGED stage=queue_migration".into());
    }
    if !source.exists() {
        record["state"] = json!("complete");
        record["recovery"] = json!("original_already_retained_or_covered");
        return save(path, &record);
    }
    if !record["converted"].is_null() {
        let wake: Wake = serde_json::from_value(record["converted"].clone())?;
        let new = record["replacement"]
            .as_str()
            .ok_or("REVIEW_CLAIM_MIGRATION_RECORD stage=queue_migration")?;
        let target = root.join("queued").join(new);
        if let Some(existing) = crate::observer_review::state::read::<Wake>(&target)? {
            if serde_json::to_value(existing)? != record["converted"] {
                return Err("REVIEW_CLAIM_MIGRATION_CHANGED stage=queue_migration".into());
            }
        } else if !archive.join(new).exists() {
            crate::observer_review::source_store::save(
                &root.join("queued"),
                new,
                &record["converted"],
            )?;
        }
        ordering::renamed(root, old, new)?;
        metadata::renamed(root, old, new)?;
        metadata::reclassified(root, new, &wake)?;
    }
    if !retired.exists() {
        fs::rename(&source, &retired)?;
    } else {
        return Err("REVIEW_CLAIM_MIGRATION_DUPLICATE stage=queue_migration".into());
    }
    fs::File::open(&archive)?.sync_all()?;
    fs::File::open(root.join("queued"))?.sync_all()?;
    record["state"] = json!("complete");
    save(path, &record)
}
pub(super) fn save(path: &Path, record: &Value) -> Result<()> {
    crate::observer_review::source_store::save(
        path.parent().unwrap(),
        path.file_name().unwrap().to_str().unwrap(),
        record,
    )
}
pub fn recover(root: &Path) -> Result<usize> {
    let directory = root.join("claim-migrations");
    if !directory.exists() {
        return Ok(0);
    }
    let mut recovered = 0;
    for entry in fs::read_dir(directory)? {
        let path = entry?.path();
        if path.extension().is_some_and(|e| e == "json") {
            let status = crate::observer_review::source_store::read_selected(
                &path,
                &crate::observer_review::source_store::Select::fields(&[(
                    "state",
                    crate::observer_review::source_store::Select::All,
                )]),
            )?;
            if status["state"] != "complete" {
                finish(root, &path)?;
                recovered += 1;
            }
        }
    }
    Ok(recovered)
}
