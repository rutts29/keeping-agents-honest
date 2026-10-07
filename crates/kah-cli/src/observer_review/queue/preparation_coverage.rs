//! Full action-set coverage and trusted authority, never a guessed time window.
use super::*;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
mod retirement;
pub use retirement::{run, stamp};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Authority {
    pub revision: Option<u64>,
    pub digest: String,
}
impl Authority {
    pub fn from_intent(intent: &Value) -> Option<Self> {
        let goal = intent["goal"].as_str()?;
        let mut items = BTreeMap::new();
        for item in intent["items"].as_array()? {
            let (id, text) = if item.is_array() {
                (item[0].as_str()?, item[1].as_str()?)
            } else {
                (item["id"].as_str()?, item["text"].as_str()?)
            };
            if items.insert(id, text).is_some() {
                return None;
            }
        }
        Some(Self {
            revision: intent["revision"].as_u64(),
            digest: format!(
                "{:x}",
                Sha256::digest(serde_json::to_vec(&json!({"goal":goal,"items":items})).ok()?)
            ),
        })
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Coverage {
    pub worker: String,
    pub reference: String,
    pub authority: Authority,
    pub action_ids: BTreeSet<String>,
}
pub fn covered(
    worker: &str,
    authority: &Authority,
    actions: &BTreeSet<String>,
    by: &Coverage,
) -> bool {
    let current = match (authority.revision, by.authority.revision) {
        (Some(old), Some(new)) => new >= old,
        _ => authority.digest == by.authority.digest,
    };
    by.worker == worker && current && !actions.is_empty() && actions.is_subset(&by.action_ids)
}
pub fn actions(values: &Value) -> Option<BTreeSet<String>> {
    let mut result = BTreeSet::new();
    for action in values.as_array()? {
        result.insert(action["id"].as_str()?.into());
    }
    (!result.is_empty()).then_some(result)
}
