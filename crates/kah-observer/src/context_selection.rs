//! One observer-selected decision context, linked to whole original values.
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Item {
    pub id: String,
    pub pointer: String,
    pub digest: String,
    pub value: Value,
    pub active_user: bool,
    pub goal: bool,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Selection {
    pub id: String,
    pub digest: String,
    pub reason: String,
    #[serde(default)]
    pub superseded_by: Option<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    pub decision: String,
    pub selected: Vec<Selection>,
    pub excluded: Vec<Selection>,
}

pub fn items(input: &Value, active_ids: &BTreeSet<String>) -> Vec<Item> {
    let mut result = vec![];
    collect(input, "", active_ids, &mut result);
    result
}
fn collect(value: &Value, pointer: &str, active: &BTreeSet<String>, out: &mut Vec<Item>) {
    if let Some(object) = value.as_object() {
        for (key, child) in object {
            let key = key.replace('~', "~0").replace('/', "~1");
            collect(child, &format!("{pointer}/{key}"), active, out);
        }
    } else if let Some(array) = value.as_array() {
        for (n, child) in array.iter().enumerate() {
            push(child, &format!("{pointer}/{n}"), active, out);
        }
    } else {
        push(value, pointer, active, out);
    }
}
fn push(value: &Value, pointer: &str, active: &BTreeSet<String>, out: &mut Vec<Item>) {
    let digest = format!("sha256:{:x}", Sha256::digest(value.to_string().as_bytes()));
    let native_id = value["id"]
        .as_str()
        .or_else(|| value.get(0).and_then(Value::as_str));
    let goal = pointer.ends_with("/goal") || pointer.ends_with("/goal/text");
    let user_text =
        pointer.ends_with("/user_text") && value.as_str().is_some_and(|s| !s.is_empty());
    out.push(Item {
        id: pointer.into(),
        pointer: pointer.into(),
        digest,
        value: value.clone(),
        goal,
        active_user: goal || user_text || native_id.is_some_and(|id| active.contains(id)),
    });
}

pub fn prepare(
    decision: &str,
    original: &Value,
    items: &[Item],
    manifest: &Manifest,
    superseders: &BTreeMap<String, BTreeSet<String>>,
) -> Result<Value, String> {
    if manifest.decision != decision {
        return Err("CONTEXT_DECISION_MISMATCH stage=context_selection".into());
    }
    let known = items
        .iter()
        .map(|i| (i.id.as_str(), i))
        .collect::<BTreeMap<_, _>>();
    let selected = manifest
        .selected
        .iter()
        .map(|s| s.id.as_str())
        .collect::<BTreeSet<_>>();
    let mut seen = BTreeSet::new();
    for (include, choices) in [(true, &manifest.selected), (false, &manifest.excluded)] {
        for choice in choices {
            let item = known
                .get(choice.id.as_str())
                .ok_or("CONTEXT_UNKNOWN_ITEM stage=context_selection")?;
            if !seen.insert(choice.id.as_str())
                || item.digest != choice.digest
                || choice.reason.trim().is_empty()
            {
                return Err("CONTEXT_ACCOUNTING stage=context_selection".into());
            }
            let replaced = choice.superseded_by.as_deref().is_some_and(|by| {
                selected.contains(by)
                    && superseders
                        .get(&choice.id)
                        .is_some_and(|ids| ids.contains(by))
            });
            if !include && (item.goal || item.active_user && !replaced) {
                return Err("CONTEXT_USER_AUTHORITY stage=context_selection".into());
            }
        }
    }
    if seen.len() != known.len() {
        return Err("CONTEXT_UNACCOUNTED_ITEMS stage=context_selection".into());
    }
    let atomic = known.keys().copied().collect();
    Ok(rebuild(original, "", &selected, &atomic).unwrap_or(Value::Null))
}
fn rebuild(
    value: &Value,
    pointer: &str,
    selected: &BTreeSet<&str>,
    atomic: &BTreeSet<&str>,
) -> Option<Value> {
    if atomic.contains(pointer) {
        return selected.contains(pointer).then(|| value.clone());
    }
    match value {
        Value::Object(map) => Some(Value::Object(
            map.iter()
                .filter_map(|(key, child)| {
                    let path = format!("{pointer}/{}", key.replace('~', "~0").replace('/', "~1"));
                    rebuild(child, &path, selected, atomic).map(|v| (key.clone(), v))
                })
                .collect(),
        )),
        Value::Array(array) => Some(Value::Array(
            array
                .iter()
                .enumerate()
                .filter_map(|(n, child)| {
                    rebuild(child, &format!("{pointer}/{n}"), selected, atomic)
                })
                .collect(),
        )),
        _ => None,
    }
}
