//! Unrelated deferrals are placed behind source links; authority stays verbatim.
use super::state::Request;
use crate::Result;
use serde_json::{Value, json};
use std::{collections::BTreeSet, path::Path};
pub(crate) fn decision(request: &Request, input: &Value, key: &str) -> Result<Value> {
    let control = super::state::read_control(&request.worker)?
        .ok_or("REVIEW_STATE_MISSING stage=decision_context")?;
    let source = control
        .observer
        .workspace
        .join("inputs")
        .join(&request.id)
        .join("intent.json");
    super::failure::validation(super::failure::Origin::ReportValidation, || {
        let state = super::decisions::scoped::report_context(input, key)?;
        let mut touched = BTreeSet::new();
        for requirement in state["decision"]["requirements"]
            .as_object()
            .into_iter()
            .flat_map(|v| v.values())
        {
            if let Some(id) = requirement["deferral_id"].as_str() {
                touched.insert(id.to_owned());
            }
        }
        if let Some(id) = state["coverage"]["requirement"]["deferral_id"].as_str() {
            touched.insert(id.to_owned());
        }
        scope(request, state, &touched, &source)
    })
}
pub fn scope(
    request: &Request,
    mut state: Value,
    touched: &BTreeSet<String>,
    source: &Path,
) -> Result<Value> {
    let all = request.intent["open_deferrals"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    for id in touched {
        if !all.iter().any(|d| d["id"] == *id) {
            return Err(format!("REVIEW_DEFERRAL_SCOPE_REFERENCE stage=decision_context: unknown linked deferral {id}").into());
        }
    }
    let mut selected = vec![];
    let mut references = vec![];
    for (index, item) in all.into_iter().enumerate() {
        let id = item["id"]
            .as_str()
            .ok_or("REVIEW_DEFERRAL_SCOPE_ID stage=decision_context")?;
        if touched.contains(id) {
            selected.push(item);
        } else {
            references.push(
                json!({"id":id,"digest":kah_observer::context_selection::digest(&item),
            "full_source":source,"pointer":format!("/open_deferrals/{index}")}),
            );
        }
    }
    state["intent"] = json!({"goal":request.intent["goal"],"goal_source":request.intent["goal_source"],
        "items":request.intent["items"],"revision":request.intent["revision"],
        "open_deferrals":selected,"other_deferrals":references,"full_source":source,
        "scope":"Whole active authority and linked deferrals; all other original intent fields remain in full_source."});
    Ok(state)
}
