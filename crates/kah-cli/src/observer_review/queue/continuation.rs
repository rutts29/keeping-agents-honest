//! A typed explicit gate continues its admitted job, retaining gate FIFO age/order.
use super::{Wake, lanes::Stamp, metadata, ordering};
use crate::Result;
use std::{collections::BTreeMap, path::Path};

pub(super) fn inherit(root: &Path, name: &str, wake: &Wake) -> Result<()> {
    let snapshot = &wake.trigger["captured_snapshot"];
    let trigger = &snapshot["trigger"];
    if !trigger["option1_cumulative_input"].is_object() {
        return Ok(());
    }
    if snapshot["head"] != trigger["option1_cumulative_input"]["head"] {
        return Err("REVIEW_CONTINUATION_HEAD stage=queue".into());
    }
    let parent = if let Some(parent) = trigger["queued_receipt"].as_str() {
        parent.to_owned()
    } else {
        // Retained pre-fix snapshots already carry original queue age. Only a
        // unique matching gate receipt is safe; ambiguity stays pending.
        let map: BTreeMap<String, Stamp> =
            super::super::state::read(&root.join("queue-metadata.json"))?.unwrap_or_default();
        let matches = map
            .iter()
            .filter(|(key, s)| {
                key.as_str() != name
                    && Some(s.at_ms) == trigger["queued_at_ms"].as_u64()
                    && s.lane == super::lanes::Lane::Gate
            })
            .map(|(key, _)| key)
            .collect::<Vec<_>>();
        if matches.len() != 1 {
            return Err("REVIEW_CONTINUATION_PARENT stage=queue".into());
        }
        matches[0].clone()
    };
    ordering::inherit(root, name, &parent)?;
    metadata::inherit(root, name, &parent)
}
