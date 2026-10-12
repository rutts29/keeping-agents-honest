//! Heavy events wait on their SHA's durable gate; independent work still runs.
use super::*;
use crate::observer_review::source_store::{Select, read_selected};
use std::{collections::BTreeSet, path::Path};

pub(super) fn filter(root: &Path, jobs: &mut Vec<(PathBuf, Wake)>) -> Result<()> {
    let mut active = BTreeSet::new();
    let checkpoint = checkpoint(root)?;
    if matches!(
        checkpoint["state"].as_str(),
        Some("verify_first_pending" | "checkpoint_needs_thinker")
    ) && let Some(head) = checkpoint["head"].as_str()
    {
        active.insert(head.to_owned());
    }
    for (directory, field) in [("gate-host", "request"), ("gate-followups", "proof")] {
        let path = root.join(directory);
        if !path.is_dir() {
            continue;
        }
        for entry in fs::read_dir(path)? {
            let path = entry?.path();
            if path.join("job.json").is_file() && !path.join("finished.json").exists() {
                let job = read_selected(
                    &path.join("job.json"),
                    &Select::fields(&[(field, Select::fields(&[("head", Select::All)]))]),
                )?;
                if let Some(head) = job[field]["head"].as_str() {
                    active.insert(head.to_owned());
                }
            }
        }
    }
    jobs.retain(|(_, wake)| {
        wake.purpose != Purpose::Checkpoint
            || wake.trigger["review_class"] == "commit"
            || wake.trigger.get("captured_snapshot").is_some()
            || wake.trigger.get("verification_followup").is_some()
            || !heads(&wake.trigger)
                .iter()
                .any(|head| active.contains(*head))
    });
    Ok(())
}
fn checkpoint(root: &Path) -> Result<Value> {
    let path = root.join("state.json");
    if fs::symlink_metadata(&path).is_err_and(|e| e.kind() == std::io::ErrorKind::NotFound) {
        return Ok(Value::Null);
    }
    Ok(read_selected(
        &path,
        &Select::fields(&[(
            "checkpoint",
            Select::fields(&[("head", Select::All), ("state", Select::All)]),
        )]),
    )?["checkpoint"]
        .clone())
}
fn heads(trigger: &Value) -> Vec<&str> {
    let mut heads = trigger["head"].as_str().into_iter().collect::<Vec<_>>();
    for event in trigger["events"].as_array().into_iter().flatten() {
        heads.extend(event["head"].as_str());
        heads.extend(event["completion_key"]["commit"].as_str());
        heads.extend(event["verification"]["commit"].as_str());
        for tag in event["tags"].as_array().into_iter().flatten() {
            heads.extend(tag.as_str().and_then(|t| t.split_whitespace().last()));
        }
    }
    heads
}
