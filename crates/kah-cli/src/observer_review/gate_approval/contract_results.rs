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
        .filter(|i| {
            i.verified && !super::contract_execution::exempted(assessment, &i.requirement_id)
        })
        .map(|i| &i.requirement_id)
        .collect::<BTreeSet<_>>();
    let needed=items.iter().filter(|i| needs_check(i,assessment)).map(|i|json!({"id":i.requirement_id,"claim":i.claim,"tried":i.tried,"receipt":i.check,"coverage":i.coverage})).collect::<Vec<_>>();
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
    assessment["unproven"] = json!(
        assessment["needs_thinker"]
            .as_array()
            .into_iter()
            .flatten()
            .chain(assessment["needs_user"].as_array().into_iter().flatten())
            .filter_map(Value::as_str)
            .chain(needed.iter().filter_map(|item| item["id"].as_str()))
            .collect::<BTreeSet<_>>()
    );
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
fn needs_check(item: &ItemResult, assessment: &Value) -> bool {
    !super::contract_execution::exempted(assessment, &item.requirement_id)
        && (item.needs_check
            || (!item.verified
                && !item.semantic_risk_admissible
                && assessment["needs_thinker"]
                    .as_array()
                    .is_some_and(|ids| ids.iter().any(|id| id == &item.requirement_id))))
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

pub(crate) fn unchecked_final(
    request: &Request,
    report: &Report,
    assessment: &mut Value,
) -> Result<()> {
    if request.trigger["verification_followup"]["stage"] != "checks_complete"
        || report
            .decision_units
            .as_ref()
            .is_none_or(|u| u.version() < 10)
    {
        return Ok(());
    }
    if !assessment["violations"]
        .as_array()
        .is_some_and(Vec::is_empty)
    {
        return Ok(());
    }
    let ids = assessment["needs_thinker"]
        .as_array()
        .ok_or("REVIEW_CONTRACT_OUTCOME stage=verification")?
        .clone();
    let results = request.trigger["verification_followup"]["item_results"].as_array();
    let mut missing = vec![];
    for id in ids {
        if !results.is_some_and(|items| items.iter().any(|i| i["requirement_id"] == id)) {
            let claim = report
                .decision_units
                .as_ref()
                .unwrap()
                .requirements
                .iter()
                .find(|r| id == r.id)
                .map(|r| r.name.clone())
                .unwrap_or_else(|| id.as_str().unwrap_or("unknown").into());
            missing.push(json!({"id":id,"claim":claim,"tried":[],"reason":"New final item has no item-bound verification receipt"}));
        }
    }
    if !missing.is_empty() {
        if !assessment["needs_check"].is_array() {
            assessment["needs_check"] = json!([]);
        }
        assessment["needs_check"]
            .as_array_mut()
            .unwrap()
            .extend(missing);
        assessment["passed"] = false.into();
        assessment["state"] = "checkpoint_needs_check".into();
    }
    Ok(())
}
