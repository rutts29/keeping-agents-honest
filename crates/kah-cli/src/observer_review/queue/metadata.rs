//! Queue mutex owns metadata; legacy age comes from retained file timestamps.
use super::*;
use lanes::{Lane, Stamp};
use std::{collections::BTreeMap, path::Path, time::UNIX_EPOCH};
type Metadata = BTreeMap<String, Stamp>;
pub(super) fn queued(root: &Path, name: &str, wake: &Wake) -> Result<()> {
    let mut map = load(root)?;
    if !map.contains_key(name) {
        let path = root.join("queued").join(name);
        let at_ms = if path.exists() {
            legacy_at(root, name, &path)?
        } else {
            kah_observer::session::now_ms()
        };
        map.insert(
            name.into(),
            Stamp {
                at_ms,
                lane: Lane::of(wake),
            },
        );
        save(root, &map)?;
    }
    Ok(())
}
pub(super) fn renamed(root: &Path, old: &str, new: &str) -> Result<()> {
    let mut map = load(root)?;
    if let Some(stamp) = map.remove(old) {
        map.insert(new.into(), stamp);
        save(root, &map)?;
    }
    Ok(())
}
pub(super) fn stamps(root: &Path, jobs: &[(PathBuf, Wake)]) -> Result<Metadata> {
    let mut map = load(root)?;
    for (path, wake) in jobs {
        let name = path.file_name().unwrap().to_str().unwrap();
        if !map.contains_key(name) {
            let at = legacy_at(root, name, path)?;
            map.insert(
                name.into(),
                Stamp {
                    at_ms: at,
                    lane: Lane::of(wake),
                },
            );
        }
        if let Some(original) = repair_age(root, name)? {
            let stamp = map.get_mut(name).unwrap();
            stamp.at_ms = stamp.at_ms.min(original);
        }
    }
    save(root, &map)?;
    Ok(map)
}
fn legacy_at(root: &Path, name: &str, path: &Path) -> Result<u64> {
    let at = modified(path)?;
    Ok(repair_age(root, name)?.map_or(at, |original| original.min(at)))
}
fn repair_age(root: &Path, name: &str) -> Result<Option<u64>> {
    let mut at: Option<u64> = None;
    let repairs = root.join("queue-repairs");
    if repairs.exists() {
        for entry in fs::read_dir(repairs)? {
            let file = entry?.path();
            if file.extension().is_some_and(|s| s == "json") {
                let record: Value = super::super::source_store::read(&file)?;
                for row in record["rows"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter(|row| row["replacement"] == name)
                {
                    let original = row["original"]
                        .as_str()
                        .ok_or("REVIEW_QUEUE_ORIGINAL stage=timing")?;
                    let old = modified(&root.join("queue-retired").join(original))?;
                    at = Some(at.map_or(old, |current| current.min(old)));
                }
            }
        }
    }
    Ok(at)
}
fn modified(path: &Path) -> Result<u64> {
    Ok(fs::metadata(path)?
        .modified()?
        .duration_since(UNIX_EPOCH)?
        .as_millis() as u64)
}
pub(super) fn last(root: &Path) -> Result<Option<Lane>> {
    let value: Option<Value> = super::super::state::read(&root.join("last-admitted-lane.json"))?;
    value
        .and_then(|v| v.get("lane").cloned())
        .map(serde_json::from_value)
        .transpose()
        .map_err(Into::into)
}
pub fn admitted(worker: &str, request: &super::super::state::Request) -> Result<()> {
    let root = Store::directory(worker)?;
    let wake = Wake {
        purpose: request.purpose.clone(),
        turn: request.turn.clone(),
        trigger: request.trigger.clone(),
    };
    let _lock = Store::delivery_at(&root.join("queue-receipts"), true)?;
    super::super::source_store::save(
        &root,
        "last-admitted-lane.json",
        &json!({"request":request.id,"lane":Lane::of(&wake),"at_ms":kah_observer::session::now_ms()}),
    )
}
fn load(root: &Path) -> Result<Metadata> {
    Ok(super::super::state::read(&root.join("queue-metadata.json"))?.unwrap_or_default())
}
fn save(root: &Path, map: &Metadata) -> Result<()> {
    super::super::source_store::save(root, "queue-metadata.json", &serde_json::to_value(map)?)
}
