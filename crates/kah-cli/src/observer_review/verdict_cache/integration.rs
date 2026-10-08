//! Consume immutable carried records; never trust thinker-supplied probabilities.
use crate::{
    Result,
    observer_review::{commit_packet, packet_references, state::Request},
};
use kah_observer::{
    decision_units::{Requirement, ReviewUnits},
    judge::{Answer, Judgment},
    review_evidence::Report,
};
use serde_json::{Value, json};
use std::collections::BTreeSet;
pub fn carried(request: &Request, report: &Report) -> Result<Vec<Value>> {
    if !commit_packet::is_commit(request)
        || request.trigger["cache_plan"]["protocol"] != "commit-cache-v1"
    {
        return Ok(vec![]);
    }
    let doc = request
        .documents
        .iter()
        .find(|d| d.id == "cached-verdicts")
        .ok_or("REVIEW_CACHE_BINDING stage=verdict_cache: carried record missing")?;
    let original = packet_references::original(doc)?;
    if original.digest != request.trigger["cache_plan"]["record_digest"] {
        return Err("REVIEW_CACHE_BINDING stage=verdict_cache: carried digest mismatch".into());
    }
    let value: Value = serde_json::from_str(&original.text)?;
    if value["worker"] != request.worker
        || value["request"] != request.id
        || value["protocol"] != "commit-cache-v1"
    {
        return Err(
            "REVIEW_CACHE_BINDING stage=verdict_cache: wrong worker/request/protocol".into(),
        );
    }
    let fresh: BTreeSet<_> = report
        .decision_units
        .iter()
        .flat_map(|u| &u.requirements)
        .map(|r| r.id.as_str())
        .collect();
    let rows = value["carried"]
        .as_array()
        .ok_or("REVIEW_CACHE_BINDING stage=verdict_cache: carried rows missing")?;
    Ok(rows
        .iter()
        .filter(|r| {
            r["requirement"]
                .as_str()
                .is_some_and(|id| !fresh.contains(id))
        })
        .cloned()
        .collect())
}
pub fn verify(request: &Request, report: &Report) -> Result<Option<Value>> {
    let Some(units) = &report.decision_units else {
        return Ok(None);
    };
    let rows = carried(request, report)?;
    let active = crate::observer_review::decisions::active_items(&request.intent);
    let covered: BTreeSet<_> = rows
        .iter()
        .flat_map(|r| {
            r["verdict"]["requirement"]["authority_items"]
                .as_array()
                .into_iter()
                .flatten()
        })
        .filter_map(Value::as_str)
        .collect();
    let required: Vec<_> = active
        .iter()
        .filter(|id| !covered.contains(id.as_str()))
        .cloned()
        .collect();
    Ok(Some(units.verify_scoped(
        &request.documents,
        &active,
        &required,
    )?))
}
pub fn units(request: &Request, report: &Report) -> Result<ReviewUnits> {
    let mut units = report
        .decision_units
        .clone()
        .ok_or("REVIEW_CACHE_UNITS stage=verdict_cache")?;
    for row in carried(request, report)? {
        let requirement: Requirement =
            serde_json::from_value(row["verdict"]["requirement"].clone())?;
        units.requirements.push(requirement);
    }
    Ok(units)
}
pub fn merge(
    request: &Request,
    report: &Report,
    judgment: &mut Judgment,
    assessment: &mut Value,
) -> Result<()> {
    let rows = carried(request, report)?;
    for row in &rows {
        let answers: std::collections::BTreeMap<String, Answer> =
            serde_json::from_value(row["verdict"]["answers"].clone())?;
        for (key, value) in answers {
            if judgment.answers.insert(key, value).is_some() {
                return Err(
                    "REVIEW_CACHE_DUPLICATE stage=verdict_cache: fresh/carried answer collision"
                        .into(),
                );
            }
        }
    }
    assessment["carried"] = json!(rows.iter().map(|r|json!({"id":r["requirement"],"review":r["carried_from_review"],"head":r["carried_from_sha"]})).collect::<Vec<_>>());
    Ok(())
}
