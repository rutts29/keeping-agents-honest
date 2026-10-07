//! Caller holds queue-receipts; every flag is retained, one pending aggregate.
use super::*;
use std::path::Path;
pub(super) const NAME: &str = "drift-pending.json";

pub(super) fn append(root: &Path, turn: &str, trigger: Value) -> Result<()> {
    let events = root.join("drift-events");
    kah_core::run_store::ensure_private_directory(&events)?;
    let event = format!("{}.json", crate::relay_store::nonce());
    let incoming = Wake {
        purpose: Purpose::Drift,
        turn: turn.into(),
        trigger,
    };
    let directory = root.join("queued");
    let previous = super::super::state::read::<Wake>(&directory.join(NAME))?;
    let mut wake = incoming.clone();
    let mut triggers = vec![];
    let mut actions = vec![];
    let mut receipts = vec![];
    let mut originals = vec![];
    if let Some(old) = previous {
        recover(root, &old)?;
        triggers = old.trigger["coalesced_triggers"]
            .as_array()
            .cloned()
            .unwrap_or_else(|| vec![old.trigger.clone()]);
        actions.extend(array(&old.trigger, "actions"));
        receipts.extend(array(&old.trigger, "event_receipts"));
        originals.extend(array(&old.trigger, "coalesced_events"));
    }
    triggers.push(incoming.trigger.clone());
    actions.extend(array(&incoming.trigger, "actions"));
    receipts.push(json!(format!("drift-events/{event}")));
    originals.push(serde_json::to_value(&incoming)?);
    wake.trigger["coalesced_triggers"] = json!(triggers);
    wake.trigger["actions"] = json!(actions);
    wake.trigger["event_receipts"] = json!(receipts);
    wake.trigger["coalesced_events"] = json!(originals);
    ordering::reserve(root, NAME)?;
    // Publish whole content first; its original record can be rebuilt after a crash.
    super::super::source_store::save(&directory, NAME, &serde_json::to_value(wake)?)?;
    super::super::source_store::save(&events, &event, &serde_json::to_value(incoming)?)
}

pub(super) fn detach(root: &Path, paths: &mut [PathBuf]) -> Result<()> {
    for path in paths {
        if path.file_name().is_some_and(|name| name == NAME) {
            let wake: Wake = super::super::source_store::read(path)?;
            recover(root, &wake)?;
            let name = format!("drift-snapshot-{}.json", crate::relay_store::nonce());
            let detached = path.with_file_name(&name);
            fs::rename(&*path, &detached)?;
            ordering::renamed(root, NAME, &name)?;
            *path = detached;
        }
    }
    Ok(())
}

fn recover(root: &Path, wake: &Wake) -> Result<()> {
    if array(&wake.trigger, "event_receipts").len()
        != array(&wake.trigger, "coalesced_events").len()
    {
        return Err("REVIEW_DRIFT_RECEIPT_MANIFEST stage=queue".into());
    }
    for (receipt, original) in wake.trigger["event_receipts"]
        .as_array()
        .into_iter()
        .flatten()
        .zip(
            wake.trigger["coalesced_events"]
                .as_array()
                .into_iter()
                .flatten(),
        )
    {
        let name = receipt
            .as_str()
            .and_then(|p| p.strip_prefix("drift-events/"))
            .filter(|n| {
                n.ends_with(".json")
                    && !n.starts_with('.')
                    && n.bytes()
                        .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'.'))
            })
            .ok_or("REVIEW_DRIFT_RECEIPT_PATH stage=queue")?;
        let directory = root.join("drift-events");
        if let Some(saved) = super::super::state::read::<Value>(&directory.join(name))? {
            if saved != *original {
                return Err("REVIEW_DRIFT_RECEIPT_CHANGED stage=queue".into());
            }
        } else {
            super::super::source_store::save(&directory, name, original)?;
        }
    }
    Ok(())
}
fn array(value: &Value, key: &str) -> Vec<Value> {
    value[key].as_array().cloned().unwrap_or_default()
}
