//! C packet scope follows checked verdict dependencies; originals stay intact.
use crate::{
    Result,
    observer_review::{commit_packet, decisions, state::Request},
};
use serde_json::{Value, json};
use std::{collections::BTreeSet, path::Path};

pub(super) fn authority(requirement: &Value) -> BTreeSet<String> {
    requirement["authority_items"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(str::to_owned)
        .collect()
}
pub(super) fn scope(request: &Request, rows: &[Value], dirty: &BTreeSet<String>) -> Value {
    let carried: BTreeSet<_> = rows
        .iter()
        .flat_map(|row| authority(&row["verdict"]["requirement"]))
        .collect();
    let fresh: Vec<_> = decisions::active_items(&request.intent)
        .into_iter()
        .filter(|id| id == "goal" || dirty.contains(id) || !carried.contains(id))
        .collect();
    json!({"carried":carried,"fresh":fresh})
}
pub fn intent(request: &Request, full_record: &Path) -> Result<Value> {
    let mut view = super::super::reminder::intent(request)?;
    if !commit_packet::is_commit(request)
        || request.trigger["cache_plan"]["protocol"] != "commit-cache-v1"
    {
        return Ok(view);
    }
    let ids = |key: &str| -> BTreeSet<&str> {
        request.trigger["cache_plan"][key]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .collect()
    };
    let carried = ids("carried_authority_items");
    let fresh = ids("fresh_authority_items");
    let mut kept = vec![];
    let mut references = vec![];
    for item in view["items"].as_array().into_iter().flatten() {
        if item["status"] != "pending_verification"
            && item["id"]
                .as_str()
                .is_some_and(|id| carried.contains(id) && !fresh.contains(id))
        {
            references.push(json!({"id":item["id"],"digest":super::capture::digest(item)?,
                "record":full_record,"kind":"unchanged user authority; verbatim original retained"}));
        } else {
            kept.push(item.clone());
        }
    }
    view["items"] = json!(kept);
    view["carried_items"] = json!(references);
    view["full_record"] = json!(full_record);
    view["carried_verdicts"] = request.trigger["cache_plan"]["carried"].clone();
    Ok(view)
}
