//! Dependency identity from already captured sources; never select/run tests.
use super::capture::digest;
use crate::{Result, observer_review::state::Request};
use kah_observer::decision_units::Requirement;
use serde_json::{Value, json};
use std::collections::BTreeMap;
pub(super) fn plans(request: &Request, r: &Requirement) -> BTreeMap<String, String> {
    request
        .documents
        .iter()
        .filter(|d| d.kind == "plan" && r.citations.iter().any(|c| c.source == d.id))
        .map(|d| (d.uri.clone(), d.digest.clone()))
        .collect()
}
pub(super) fn components(request: &Request, r: &Requirement) -> Result<BTreeMap<String, String>> {
    if r.components.is_empty() {
        return Ok(BTreeMap::new());
    }
    let project: Value = request
        .documents
        .iter()
        .find(|d| d.id == "git" && d.kind == "controller_facts")
        .map(|d| serde_json::from_str(&d.text))
        .transpose()?
        .ok_or("REVIEW_CACHE_COMPONENT stage=verdict_cache: native inventory missing")?;
    let expected = &project["component_map_binding"]["map_digest"];
    if expected.is_null() || *expected != project["component_impact"]["declaration_digest"] {
        return Err(
            "REVIEW_CACHE_COMPONENT stage=verdict_cache: checked native map binding missing".into(),
        );
    }
    let components = project["component_impact"]["components"]
        .as_array()
        .ok_or("REVIEW_CACHE_COMPONENT stage=verdict_cache: native components missing")?;
    r.components.iter().map(|component| {
        let found = components.iter().find(|c| c["name"] == component.name
            && c["requirements"].as_array().is_some_and(|ids| ids.iter().any(|id| id == &r.id)))
            .ok_or_else(|| format!("REVIEW_CACHE_COMPONENT stage=verdict_cache: requirement {:?}, unmapped component {:?}",r.id,component.name))?;
        let native = found["digest"].as_str().ok_or("REVIEW_CACHE_COMPONENT stage=verdict_cache: native digest missing")?;
        Ok((component.name.clone(),digest(&json!({"native":native,"role":component.role}))?))
    }).collect()
}
pub(super) fn tests(request: &Request, r: &Requirement) -> Result<BTreeMap<String, String>> {
    let latest = super::receipts::latest(request, r)?;
    r.checks
        .iter()
        .map(|check| {
            let selected = [
                check.command_citation,
                check.result_citation,
                check.sha_citation,
            ]
            .iter()
            .map(|i| {
                r.citations
                    .get(*i)
                    .map(|c| json!({"source":c.source,"quote":c.quote,"occurrence":c.occurrence}))
                    .ok_or("REVIEW_CACHE_CHECK stage=verdict_cache: check citation missing")
            })
            .collect::<std::result::Result<Vec<_>, _>>()?;
            Ok((
                check.name.clone(),
                digest(&json!({"check":check,"receipts":selected,"native_execution":latest[&check.name]}))?,
            ))
        })
        .collect()
}
