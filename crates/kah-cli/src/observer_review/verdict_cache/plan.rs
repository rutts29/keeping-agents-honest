//! Exact evidence invalidation; no test selection, execution or model summary.
use super::{Entry, capture};
use crate::{
    Result,
    observer_review::{commit_packet, state::Request},
};
use kah_observer::{decision_units::Requirement, review_evidence::VerifiedSources};
use serde_json::Value;
use serde_json::json;
use std::path::Path;
pub fn prepare(private: &Path, request: &mut Request) -> Result<()> {
    if !commit_packet::is_commit(request) {
        return Ok(());
    }
    let root = private.join("verdict-cache");
    let entries = match std::fs::read_dir(&root) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error.into()),
    };
    let mut saved = vec![];
    for path in entries {
        let path = path?.path();
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        let Some(entry) = crate::observer_review::state::read::<Entry>(&path)? else {
            continue;
        };
        saved.push(entry);
    }
    expand_code_sources(request, &saved)?;
    let mut carried = vec![];
    let mut dirty = vec![];
    for entry in saved {
        match candidate(request, &entry)? {
            Some(value) => carried.push(value),
            None => dirty.push(entry.requirement),
        }
    }
    let id = "cached-verdicts";
    let record = kah_observer::review_evidence::Document::new(
        id,
        "controller_cached_verdicts",
        "kah:checked-cache",
        &serde_json::to_string(
            &json!({"protocol":"commit-cache-v1","worker":request.worker,"request":request.id,"carried":carried}),
        )?,
    );
    request.trigger["cache_plan"] = json!({"protocol":"commit-cache-v1","dirty":dirty,
        "carried":carried.iter().map(|v|json!({"id":v["requirement"],"review":v["carried_from_review"],"head":v["carried_from_sha"]})).collect::<Vec<_>>(),
        "record_source":id,"record_digest":record.digest,"always":"intent overview,drift,prohibitions,claim_made,proportion,user waits"});
    request.documents.push(record);
    Ok(())
}
fn expand_code_sources(request: &mut Request, entries: &[Entry]) -> Result<()> {
    let sources: std::collections::BTreeSet<_> = entries
        .iter()
        .flat_map(|e| {
            e.verdict["requirement"]["citations"]
                .as_array()
                .into_iter()
                .flatten()
        })
        .filter_map(|c| c["source"].as_str())
        .filter(|id| {
            id.starts_with("commit-file-") && !request.documents.iter().any(|d| d.id == *id)
        })
        .collect();
    let hints =
        json!({"sources":sources.iter().map(|source|json!({"source":source})).collect::<Vec<_>>()});
    let added = commit_packet::expand(request, &hints)?;
    request.trigger["cache_source_ids"] = json!(added);
    Ok(())
}
#[cfg(test)]
#[path = "../../../tests/observer_unit/cache_planning.rs"]
mod tests;

pub fn candidate(request: &Request, entry: &Entry) -> Result<Option<Value>> {
    if !commit_packet::is_commit(request)
        || !entry.confirmed
        || entry.always
        || entry.verdict["verified"].is_null()
        || entry.verdict["protocol"] != kah_observer::decision_units::VERSION
    {
        return Ok(None);
    }
    if request
        .documents
        .iter()
        .find(|d| d.id == "git")
        .map(|d| serde_json::from_str::<Value>(&d.text))
        .transpose()?
        .is_some_and(|project| {
            project["component_impact"]["unmapped_paths"]
                .as_array()
                .is_some_and(|paths| !paths.is_empty())
        })
    {
        return Ok(None);
    }
    let requirement: Requirement = serde_json::from_value(entry.verdict["requirement"].clone())?;
    // Temporary WIP guard: old receipts remain in append-only history.
    // Until latest-result identity is bound, test-backed items stay fresh.
    if !requirement.checks.is_empty() {
        return Ok(None);
    }
    if super::always(
        requirement.dimension,
        requirement
            .applicability
            .as_ref()
            .is_some_and(|a| a.unconditional),
        false,
        requirement.existing_question_id.is_some()
            || matches!(
                requirement.judgment,
                kah_observer::decision_units::Thinking::NeedsUser { .. }
            ),
    ) {
        return Ok(None);
    }
    let Some(current) = current_requirement(request, &requirement)? else {
        return Ok(None);
    };
    match capture::fingerprint(request, &current) {
        Ok(evidence) => refresh_verified(request, entry, &current, &evidence),
        Err(error)
            if error.to_string().starts_with("REVIEW_CACHE_AUTHORITY")
                || error.to_string().starts_with("REVIEW_CACHE_SOURCE")
                || error.to_string().starts_with("REVIEW_CACHE_COMPONENT") =>
        {
            Ok(None)
        }
        Err(error) => Err(error),
    }
}
fn refresh_verified(
    request: &Request,
    entry: &Entry,
    current: &Requirement,
    evidence: &super::Evidence,
) -> Result<Option<Value>> {
    let Some(mut carried) = entry.carry(evidence, false) else {
        return Ok(None);
    };
    let units = kah_observer::decision_units::ReviewUnits {
        version: Some(kah_observer::decision_units::VERSION),
        requirements: vec![current.clone()],
        proportion: None,
        steering: None,
    };
    let verified = match units.verify(&request.documents, &current.authority_items) {
        Ok(verified) => verified,
        Err(_) => return Ok(None),
    };
    carried["verdict"]["requirement"] = serde_json::to_value(current)?;
    carried["verdict"]["verified"] = verified["requirements"][0].clone();
    Ok(Some(carried))
}
fn current_requirement(
    request: &Request,
    requirement: &Requirement,
) -> Result<Option<Requirement>> {
    let mut current = requirement.clone();
    let sources = VerifiedSources::new(&request.documents);
    for citation in &mut current.citations {
        let Some(document) = request.documents.iter().find(|d| d.id == citation.source) else {
            return Ok(None);
        };
        citation.digest = document.digest.clone();
        if sources.quote(citation).is_err() {
            return Ok(None);
        }
    }
    Ok(Some(current))
}
