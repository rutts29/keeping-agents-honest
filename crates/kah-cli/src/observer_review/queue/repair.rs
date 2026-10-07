//! User-authorized one-time compaction, preserving original files and CAS sources.
use super::*;
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, path::Path};

pub fn backlog(worker: &str, authorization: &str, checkpoint: Wake) -> Result<Value> {
    if authorization.is_empty() || checkpoint.purpose != Purpose::Checkpoint {
        return Err("REVIEW_BACKLOG_AUTHORIZATION stage=queue_repair".into());
    }
    let root = Store::directory(worker)?;
    let _lock = Store::delivery_at(&root.join("queue-receipts"), true)?;
    let repair = format!("backlog-{:x}", Sha256::digest(authorization.as_bytes()));
    let record = root.join("queue-repairs").join(format!("{repair}.json"));
    if let Some(saved) = super::super::state::read::<Value>(&record)? {
        return finish(&root, &record, saved);
    }
    let (rows, replacements) = plan(&root, &repair, checkpoint)?;
    let value = json!({"authorization":authorization,"repair":repair,"state":"planned","rows":rows,"replacements":replacements});
    kah_core::run_store::ensure_private_directory(record.parent().unwrap())?;
    super::super::source_store::save(
        record.parent().unwrap(),
        record.file_name().unwrap().to_str().unwrap(),
        &value,
    )?;
    finish(&root, &record, value)
}

fn plan(
    root: &Path,
    repair: &str,
    checkpoint: Wake,
) -> Result<(Vec<Value>, BTreeMap<String, Wake>)> {
    let checkpoint_name = format!("{repair}-checkpoint.json");
    let drift_name = drift::NAME.to_owned();
    let mut replacements = BTreeMap::from([(checkpoint_name.clone(), checkpoint)]);
    let mut rows = vec![];
    let directory = root.join("queued");
    let mut paths = fs::read_dir(&directory)?
        .map(|e| e.map(|e| e.path()))
        .collect::<std::io::Result<Vec<_>>>()?;
    paths.sort();
    let mut drifts = vec![];
    for path in paths
        .into_iter()
        .filter(|p| p.extension().is_some_and(|e| e == "json"))
    {
        let bytes = fs::read(&path)?;
        let raw: Value = serde_json::from_slice(&bytes)?;
        let purpose: Purpose = serde_json::from_value(raw["purpose"].clone())?;
        let replacement = match purpose {
            Purpose::Steering => {
                let wake: Wake = super::super::source_store::read(&path)?;
                let name = identity::steering(&wake.trigger)?
                    .ok_or("REVIEW_BACKLOG_STEERING_SOURCE stage=queue_repair")?;
                replacements.entry(name.clone()).or_insert(wake);
                name
            }
            Purpose::Drift => {
                let wake: Wake = super::super::source_store::read(&path)?;
                if wake.trigger.get("captured_snapshot").is_some() {
                    return Err("REVIEW_BACKLOG_SNAPSHOT stage=queue_repair: preserve this explicit snapshot independently".into());
                }
                drifts.push((path.clone(), wake));
                drift_name.clone()
            }
            _ => checkpoint_name.clone(),
        };
        rows.push(json!({"original":path.file_name().unwrap().to_str().unwrap(),"purpose":purpose,"digest":format!("{:x}",Sha256::digest(&bytes)),"disposition":if matches!(purpose,Purpose::Steering|Purpose::Drift){"covered_by"}else{"superseded"},"replacement":replacement}));
    }
    if let Some((wake, _)) = batch(&drifts) {
        replacements.insert(drift_name, wake);
    }
    Ok((rows, replacements))
}

fn finish(root: &Path, record: &Path, mut value: Value) -> Result<Value> {
    if value["state"] == "complete" {
        return Ok(value);
    }
    let retired = root.join("queue-retired");
    kah_core::run_store::ensure_private_directory(&retired)?;
    for row in value["rows"]
        .as_array()
        .ok_or("REVIEW_BACKLOG_MANIFEST stage=queue_repair")?
    {
        let name = row["original"]
            .as_str()
            .ok_or("REVIEW_BACKLOG_MANIFEST stage=queue_repair")?;
        let source = root.join("queued").join(name);
        let target = retired.join(name);
        let existing = if target.exists() { &target } else { &source };
        let digest = format!("{:x}", Sha256::digest(fs::read(existing)?));
        if row["digest"] != digest {
            return Err("REVIEW_BACKLOG_SOURCE_CHANGED stage=queue_repair".into());
        }
        if !target.exists() {
            fs::rename(&source, &target)?;
        }
    }
    let replacements: BTreeMap<String, Wake> =
        serde_json::from_value(value["replacements"].clone())?;
    for (name, wake) in &replacements {
        let replacement = serde_json::to_value(wake)?;
        if let Some(existing) = super::super::state::read::<Value>(&root.join("queued").join(name))?
            && existing != replacement
        {
            return Err(format!("REVIEW_BACKLOG_CONCURRENT_ENTRY stage=queue_repair: {name}; both original archives and new content retained, never overwritten").into());
        }
        ordering::reserve(root, name)?;
        super::super::source_store::save(&root.join("queued"), name, &replacement)?;
    }
    value["state"] = "complete".into();
    value["before"] = json!(value["rows"].as_array().unwrap().len());
    value["after"] = json!(replacements.len());
    super::super::source_store::save(
        record.parent().unwrap(),
        record.file_name().unwrap().to_str().unwrap(),
        &value,
    )?;
    Ok(value)
}
