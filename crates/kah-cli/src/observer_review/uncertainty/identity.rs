//! Repeated delivery metadata is not new evidence for an unresolved decision.
use crate::{
    Result,
    observer_review::{source_store, state::Request},
};
use kah_observer::review_evidence::Report;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{collections::BTreeSet, path::Path};

pub fn fresh_followup(
    root: &Path,
    request: &Request,
    report: &Report,
    assessment: &Value,
) -> Result<bool> {
    if request.head.is_none() || request.purpose != super::super::state::Purpose::Checkpoint {
        return Ok(true);
    }
    let Some(units) = &report.decision_units else {
        return Ok(true);
    };
    let ids = assessment["needs_thinker"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .collect::<BTreeSet<_>>();
    if ids.is_empty() {
        return Ok(true);
    }
    let verified = units.verify(
        &request.documents,
        &super::super::decisions::active_items(&request.intent),
    )?;
    let evidence = evidence(units, &verified, &ids)?;
    let identity = json!({"worker":request.worker,"head":request.head,"goal":request.intent["goal"],"items":request.intent["items"],"evidence":evidence});
    let digest = format!("{:x}", Sha256::digest(serde_json::to_vec(&identity)?));
    kah_core::run_store::ensure_private_directory(&root.join("sources"))?;
    let directory = root.join("followup-identities");
    kah_core::run_store::ensure_private_directory(&directory)?;
    kah_core::run_store::ensure_private_directory(&root.join("followup-events"))?;
    let path = directory.join(format!("{digest}.json"));
    let previous: Option<Value> = super::super::state::read(&path)?;
    // The complete original reports remain in results/CAS. This compares exact
    // whole quotes, not growing source-prefix digests, receipt locations or turns.
    source_store::save(
        &root.join("followup-events"),
        &format!("{}.json", request.id),
        &json!({"request":request.id,"identity":digest,"original":previous.as_ref().map(|v|v["request"].clone()),"unchanged":previous.is_some()}),
    )?;
    if let Some(previous) = previous {
        // Continue the already-admitted one-round lifecycle after a crash or
        // its explicit reconsideration; only a NEW automatic cycle is blocked.
        return Ok(previous["request"] == request.id
            || previous["request"] == request.trigger["reconsideration"]["request"]);
    }
    source_store::save(
        &directory,
        &format!("{digest}.json"),
        &json!({"request":request.id,"identity":identity}),
    )?;
    Ok(true)
}

fn evidence(
    units: &kah_observer::decision_units::ReviewUnits,
    verified: &Value,
    ids: &BTreeSet<&str>,
) -> Result<Vec<Value>> {
    let mut evidence = vec![];
    for id in ids {
        let requirement = units
            .requirements
            .iter()
            .find(|r| r.id == *id)
            .ok_or("REVIEW_RETHINK_REQUIREMENT stage=rethink_identity")?;
        let value = serde_json::to_value(requirement)?;
        let mut meaning = value["judgment"].clone();
        meaning.as_object_mut().unwrap().remove("reasoning");
        meaning
            .as_object_mut()
            .unwrap()
            .remove("evidence_citations");
        let indices = value["judgment"]["evidence_citations"]
            .as_array()
            .into_iter()
            .flatten()
            .chain(
                value["applicability"]["evidence_citations"]
                    .as_array()
                    .into_iter()
                    .flatten(),
            )
            .filter_map(Value::as_u64)
            .map(|i| i as usize)
            .chain(
                requirement
                    .checks
                    .iter()
                    .flat_map(|c| [c.command_citation, c.result_citation, c.sha_citation]),
            )
            .collect::<BTreeSet<_>>();
        let mut quotes = BTreeSet::new();
        for index in indices {
            let checked = verified["requirements"]
                .as_array()
                .unwrap()
                .iter()
                .find(|r| r["id"] == *id)
                .and_then(|r| r["citations"].get(index))
                .ok_or("REVIEW_RETHINK_CITATION stage=rethink_identity")?;
            quotes.insert(serde_json::to_string(&quote_identity(checked))?);
        }
        evidence.push(json!({"id":id,"authority":requirement.authority_items,"quotes":quotes,"checks":requirement.checks.iter().map(|c|json!({"name":c.name,"sha":c.sha})).collect::<Vec<_>>(),
            "meaning":meaning,"due":value["applicability"]["due"]}));
    }
    Ok(evidence)
}

fn quote_identity(checked: &Value) -> Value {
    let mut quote = checked["quote"].clone();
    if matches!(
        checked["kind"].as_str(),
        Some("controller_event" | "controller_completion_due")
    ) && let Some(text) = quote.as_str()
        && let Ok(mut value) = serde_json::from_str::<Value>(text)
        && value.is_object()
    {
        // Request linkage is preserved in the original checked receipt. A new
        // rendering of the same host fact is not new substantive evidence.
        value.as_object_mut().unwrap().remove("request");
        for receipt in value["receipts"].as_array_mut().into_iter().flatten() {
            if let Some(fields) = receipt.as_object_mut() {
                fields.remove("request");
            }
        }
        quote = value;
    }
    json!({"uri":checked["uri"],"kind":checked["kind"],"quote":quote})
}
