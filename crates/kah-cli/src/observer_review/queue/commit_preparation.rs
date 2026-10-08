//! Reviewer-approved commit-only preparation. Originals remain in the queue.
use super::*;
use std::{collections::BTreeMap, path::Path};

#[derive(Serialize, Deserialize)]
struct Boundary {
    head: String,
    through_sequence: u64,
    at_ms: u64,
}

pub(super) fn advance(root: &Path, purpose: &Purpose, trigger: &Value) -> Result<()> {
    if *purpose != Purpose::Checkpoint {
        return Ok(());
    }
    let Some(head) =
        supersede::trigger_head(trigger).or_else(|| trigger["captured_snapshot"]["head"].as_str())
    else {
        return Ok(());
    };
    let prior: Option<Boundary> =
        super::super::state::read(&root.join("preparation-commit-boundary.json"))?;
    if prior.as_ref().is_some_and(|previous| previous.head == head) {
        return Ok(());
    }
    let paths = fs::read_dir(root.join("queued"))?
        .map(|entry| entry.map(|entry| entry.path()))
        .collect::<std::io::Result<Vec<_>>>()?;
    let paths = paths
        .into_iter()
        .filter(|path| path.extension().is_some_and(|e| e == "json"))
        .collect::<Vec<_>>();
    let order = ordering::record(root, &paths)?;
    let boundary = Boundary {
        head: head.into(),
        through_sequence: order.values().copied().max().unwrap_or(0),
        at_ms: kah_observer::session::now_ms(),
    };
    super::super::source_store::save(
        root,
        "preparation-commit-boundary.json",
        &serde_json::to_value(boundary)?,
    )
}

pub(super) fn eligible(root: &Path, jobs: &mut Vec<(PathBuf, Wake)>) -> Result<()> {
    let boundary: Option<Boundary> =
        super::super::state::read(&root.join("preparation-commit-boundary.json"))?;
    let order: BTreeMap<String, u64> =
        super::super::state::read(&root.join("queue-order.json"))?.unwrap_or_default();
    jobs.retain(|(path, wake)| {
        if wake.purpose != Purpose::Preparation || wake.trigger.get("captured_snapshot").is_some() {
            return true;
        }
        boundary.as_ref().is_some_and(|b| {
            path.file_name()
                .and_then(|name| name.to_str())
                .and_then(|name| order.get(name))
                .is_some_and(|sequence| *sequence <= b.through_sequence)
        })
    });
    Ok(())
}
