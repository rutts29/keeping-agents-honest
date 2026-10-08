//! Native, privately bound map attribution; no test selection or execution.
use crate::{Result, observer_review::state::Request};
use serde_json::{Value, json};
use std::collections::BTreeSet;

pub(super) fn read(request: &Request) -> Result<Value> {
    let project = request
        .documents
        .iter()
        .find(|d| d.id == "git" && d.kind == "controller_facts")
        .map(|d| serde_json::from_str::<Value>(&d.text))
        .transpose()?
        .unwrap_or(Value::Null);
    let impact = &project["component_impact"];
    let requirements: BTreeSet<_> = impact["components"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|component| {
            component["changed_paths"]
                .as_array()
                .is_some_and(|paths| !paths.is_empty())
        })
        .flat_map(|component| component["requirements"].as_array().into_iter().flatten())
        .filter_map(Value::as_str)
        .collect();
    Ok(json!({"requirements":requirements,"unattributed_paths":impact["unmapped_paths"]}))
}
