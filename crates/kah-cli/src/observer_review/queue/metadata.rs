//! Queue mutex owns metadata; legacy age comes from retained file timestamps.
use super::*;
use lanes::{Lane, Stamp};
use std::{collections::BTreeMap, path::Path, time::UNIX_EPOCH};
type Metadata = BTreeMap<String, Stamp>;
pub(super) fn reclassified(root: &Path, name: &str, wake: &Wake) -> Result<()> {
    let mut map = load(root)?;
    if let Some(stamp) = map.get_mut(name) {
        stamp.lane = Lane::of(wake);
    }
    save(root, &map)
}
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
    let repaired = repair_ages(root)?;
    let mut changed = false;
    for (path, wake) in jobs {
        let name = path.file_name().unwrap().to_str().unwrap();
        if !map.contains_key(name) {
            let at = modified(path)?;
            map.insert(
                name.into(),
                Stamp {
                    at_ms: repaired.get(name).copied().map_or(at, |old| old.min(at)),
                    lane: Lane::of(wake),
                },
            );
            changed = true;
        }
        if let Some(original) = repaired.get(name) {
            let stamp = map.get_mut(name).unwrap();
            if *original < stamp.at_ms {
                stamp.at_ms = *original;
                changed = true;
            }
        }
    }
    if changed {
        save(root, &map)?;
    }
    Ok(map)
}

fn legacy_at(root: &Path, name: &str, path: &Path) -> Result<u64> {
    let at = modified(path)?;
    Ok(repair_age(root, name)?.map_or(at, |original| original.min(at)))
}
fn repair_age(root: &Path, name: &str) -> Result<Option<u64>> {
    Ok(repair_ages(root)?.get(name).copied())
}
fn repair_ages(root: &Path) -> Result<BTreeMap<String, u64>> {
    let mut result = BTreeMap::new();
    let repairs = root.join("queue-repairs");
    if !repairs.exists() {
        return Ok(result);
    }
    for entry in fs::read_dir(repairs)? {
        let file = entry?.path();
        if file.extension().is_none_or(|s| s != "json") {
            continue;
        }
        let raw: Value = serde_json::from_slice(&fs::read(&file)?)?;
        for row in raw["rows"].as_array().into_iter().flatten() {
            // Only link metadata is needed. Never decode the archived wake/state
            // or every replacement payload while holding the admission lock.
            let replacement = super::super::source_store::expand(
                row["replacement"].clone(),
                &root.join("sources"),
            )?;
            let original =
                super::super::source_store::expand(row["original"].clone(), &root.join("sources"))?;
            let name = replacement
                .as_str()
                .ok_or("REVIEW_QUEUE_REPLACEMENT stage=timing")?;
            let original = original
                .as_str()
                .ok_or("REVIEW_QUEUE_ORIGINAL stage=timing")?;
            let at = modified(&root.join("queue-retired").join(original))?;
            result
                .entry(name.to_owned())
                .and_modify(|old: &mut u64| *old = (*old).min(at))
                .or_insert(at);
        }
    }
    Ok(result)
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
    super::super::timing::now(worker, &request.id, "admitted")?;
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
