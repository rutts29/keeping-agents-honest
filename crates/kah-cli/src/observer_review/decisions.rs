//! Verify the optional v4 report against the complete protected authority set.
use super::state::Request;
use crate::Result;
use kah_observer::review_evidence::Report;
use serde_json::Value;

pub fn active_items(intent: &Value) -> Vec<String> {
    let mut items = Vec::new();
    if !intent["goal"].is_null() {
        items.push("goal".into());
    }
    if let Some(authority) = intent["items"].as_array() {
        items.extend(
            authority
                .iter()
                .filter_map(|item| item["id"].as_str().map(str::to_owned)),
        );
    }
    items
}
pub fn verify(request: &Request, report: &Report) -> Result<Option<Value>> {
    report
        .verify_units(&request.documents, &active_items(&request.intent))
        .map_err(Into::into)
}
