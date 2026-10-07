//! Checked item outcomes override verification uncertainty, never violations.
use super::contract_execution::ItemResult;
use super::*;
use std::collections::BTreeSet;

pub(super) fn apply(root: &Path, request: &Request, assessment: &mut Value) -> Result<()> {
    if request.trigger["verification_followup"]["stage"] != "checks_complete" {
        return Ok(());
    }
    let Some(items) = request.trigger["verification_followup"]["item_results"].as_array() else {
        return Ok(());
    };
    if items.is_empty() {
        return Ok(());
    }
    let expected = super::followup::origin(root, request)?;
    let items = items
        .iter()
        .cloned()
        .map(serde_json::from_value::<ItemResult>)
        .collect::<std::result::Result<Vec<_>, _>>()?;
    let saved: Vec<ItemResult> = super::super::source_store::read(
        &root
            .join("gate-followups")
            .join(expected.id()?)
            .join("item-results.json"),
    )?;
    if serde_json::to_value(&saved)? != serde_json::to_value(&items)? {
        return Err("REVIEW_CONTRACT_BINDING stage=verification: changed item receipts".into());
    }
    if items.iter().any(|item| item.binding != expected) {
        return Err("REVIEW_CONTRACT_BINDING stage=verification".into());
    }
    apply_items(&items, assessment)
}
fn apply_items(items: &[ItemResult], assessment: &mut Value) -> Result<()> {
    let verified = items
        .iter()
        .filter(|i| i.verified)
        .map(|i| &i.requirement_id)
        .collect::<BTreeSet<_>>();
    let needed=items.iter().filter(|i|i.needs_check).map(|i|json!({"id":i.requirement_id,"claim":i.claim,"tried":i.tried,"receipt":i.check,"coverage":i.coverage})).collect::<Vec<_>>();
    if !assessment["violations"]
        .as_array()
        .is_some_and(Vec::is_empty)
    {
        return Ok(());
    }
    let ids = assessment["needs_thinker"]
        .as_array_mut()
        .ok_or("REVIEW_CONTRACT_OUTCOME stage=verification")?;
    ids.retain(|id| {
        !id.as_str()
            .is_some_and(|id| verified.iter().any(|v| v.as_str() == id))
    });
    assessment["contract_verified"] = json!(verified);
    assessment["needs_check"] = json!(needed);
    if !needed.is_empty() {
        assessment["passed"] = false.into();
        assessment["state"] = "checkpoint_needs_check".into();
    } else if assessment["needs_thinker"]
        .as_array()
        .is_some_and(Vec::is_empty)
        && assessment["needs_user"]
            .as_array()
            .is_some_and(Vec::is_empty)
    {
        assessment["passed"] = true.into();
        assessment["state"] = "checkpoint_passed".into();
    }
    Ok(())
}
pub(super) fn send_back(root: &Path, request: &Request, assessment: &Value) -> Result<bool> {
    let Some(items) = assessment["needs_check"]
        .as_array()
        .filter(|a| !a.is_empty())
    else {
        return Ok(false);
    };
    let round = super::followup::start(root, request)?;
    let path = root.join("gate-followups").join(&round.id);
    super::super::source_store::save(
        &path,
        "finished.json",
        &json!({"status":"needs_check","request":request.id,"items":items,"automatic_user_question":false}),
    )?;
    super::contract_delivery::deliver(root, request, &round, items)?;
    Ok(true)
}

#[cfg(test)]
#[path = "../../../tests/observer_unit/v10_contract_results.rs"]
mod tests;
