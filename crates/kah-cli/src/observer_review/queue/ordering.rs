//! Durable publish order; callers hold the existing queue-receipts lock.
use super::*;
use std::{collections::BTreeMap, path::Path};
type Order = BTreeMap<String, u64>;

pub(super) fn inherit(root: &Path, name: &str, parent: &str) -> Result<()> {
    let mut order: Order =
        super::super::state::read(&root.join("queue-order.json"))?.unwrap_or_default();
    let sequence = *order
        .get(parent)
        .ok_or("REVIEW_CONTINUATION_ORDER stage=queue")?;
    order.insert(name.into(), sequence);
    super::super::source_store::save(root, "queue-order.json", &serde_json::to_value(order)?)
}

pub(super) fn renamed(root: &Path, old: &str, new: &str) -> Result<()> {
    let mut order: Order =
        super::super::state::read(&root.join("queue-order.json"))?.unwrap_or_default();
    if let Some(sequence) = order.remove(old) {
        order.insert(new.into(), sequence);
    }
    super::super::source_store::save(root, "queue-order.json", &serde_json::to_value(order)?)
}

pub(super) fn reserve(root: &Path, name: &str) -> Result<()> {
    let mut order: Order =
        super::super::state::read(&root.join("queue-order.json"))?.unwrap_or_default();
    if !order.contains_key(name) {
        let next = order
            .values()
            .max()
            .copied()
            .unwrap_or(0)
            .checked_add(1)
            .ok_or("REVIEW_QUEUE_SEQUENCE stage=queue")?;
        order.insert(name.into(), next);
        super::super::source_store::save(root, "queue-order.json", &serde_json::to_value(order)?)?;
    }
    Ok(())
}
pub(super) fn record(root: &Path, paths: &[PathBuf]) -> Result<Order> {
    let mut legacy = vec![];
    for path in paths {
        match fs::metadata(path).and_then(|m| m.modified()) {
            Ok(time) => legacy.push((time, path)),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
    }
    legacy.sort();
    // Older releases recorded no sequence. Preserve their available chronology
    // once, then never use mtimes for these entries again. No trigger is removed.
    for (_, path) in legacy {
        reserve(root, path.file_name().unwrap().to_str().unwrap())?;
    }
    for path in paths {
        reserve(root, path.file_name().unwrap().to_str().unwrap())?;
    }
    Ok(super::super::state::read(&root.join("queue-order.json"))?.unwrap_or_default())
}
