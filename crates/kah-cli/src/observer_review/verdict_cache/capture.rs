//! Capture only already validated decisions; caching never supplies authority.
use super::{Entry, Evidence, always};
use crate::{
    Result,
    observer_review::{
        source_store,
        state::{Request, Store},
    },
};
use kah_observer::{
    decision_units::{self, Thinking},
    judge::Judgment,
    review_evidence::Report,
};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
#[cfg(test)]
#[path = "../../../tests/observer_unit/cache_dependencies.rs"]
mod dependency_tests;

pub fn record(
    store: &Store,
    request: &Request,
    report: &Report,
    judgment: &Judgment,
) -> Result<()> {
    if !super::super::commit_packet::is_commit(request) {
        return Ok(());
    }
    let Some(head) = &request.head else {
        return Ok(());
    };
    let Some(units) = &report.decision_units else {
        return Ok(());
    };
    let outcome = decision_units::assess(units, judgment);
    let verified = super::integration::verify(request, report)?
        .ok_or("REVIEW_CACHE_VERIFIED stage=verdict_cache: checked units missing")?;
    let mut coverage = serde_json::to_value(&outcome)?;
    super::super::light_commit::coverage(request, report, &mut coverage);
    let root = store.directory.join("verdict-cache");
    kah_core::run_store::ensure_private_directory(&root)?;
    for requirement in &units.requirements {
        let mut entry = entry(request, report, judgment, &outcome, requirement, head)?;
        entry.verdict["verified"] = verified["requirements"]
            .as_array()
            .and_then(|items| items.iter().find(|item| item["id"] == requirement.id))
            .cloned()
            .ok_or("REVIEW_CACHE_VERIFIED stage=verdict_cache: checked requirement missing")?;
        entry.verdict["backend"] = json!(judgment.backend);
        entry.verdict["model"] = json!(judgment.model);
        if coverage["needs_check"]
            .as_array()
            .is_some_and(|items| items.iter().any(|i| i == &requirement.id))
        {
            entry.confirmed = false;
        }
        source_store::save(
            &root,
            &format!("{}.json", digest(&json!(requirement.id))?),
            &serde_json::to_value(entry)?,
        )?;
    }
    Ok(())
}
fn entry(
    request: &Request,
    report: &Report,
    judgment: &Judgment,
    outcome: &decision_units::Outcome,
    requirement: &decision_units::Requirement,
    head: &str,
) -> Result<Entry> {
    let id = &requirement.id;
    let (evidence, eligibility_gap) = snapshot(request, requirement)?;
    let confirmed = eligibility_gap.is_none()
        && !outcome.needs_thinker.contains(id)
        && !outcome.needs_user.contains(id)
        && !outcome.not_due.contains(id);
    let unconditional = requirement
        .applicability
        .as_ref()
        .is_some_and(|a| a.unconditional);
    let claim = report.findings.iter().any(|f| {
        matches!(f.topic, kah_observer::review_evidence::Topic::Claim)
            && f.citations.iter().any(|c| {
                requirement
                    .citations
                    .iter()
                    .any(|r| r.source == c.source && r.quote == c.quote)
            })
    });
    let answers: BTreeMap<_, _> = judgment
        .answers
        .iter()
        .filter(|(key, _)| key.ends_with(&format!(".{id}")))
        .map(|(key, value)| (key.clone(), value.clone()))
        .collect();
    Ok(Entry {
        requirement: id.clone(),
        review: request.id.clone(),
        head: head.into(),
        evidence,
        confirmed,
        always: always(
            requirement.dimension,
            unconditional,
            claim,
            matches!(requirement.judgment, Thinking::NeedsUser { .. })
                || requirement.existing_question_id.is_some(),
        ),
        verdict: json!({"requirement":requirement,"answers":answers,"protocol":report.decision_units.as_ref().unwrap().version(),"eligibility_gap":eligibility_gap}),
    })
}
pub(super) fn fingerprint(request: &Request, r: &decision_units::Requirement) -> Result<Evidence> {
    let mut authority = BTreeMap::new();
    for id in &r.authority_items {
        let value = if id == "goal" {
            &request.intent["goal"]
        } else {
            request.intent["items"]
                .as_array()
                .and_then(|items| items.iter().find(|i| i["id"] == *id))
                .ok_or("REVIEW_CACHE_AUTHORITY stage=verdict_cache")?
        };
        authority.insert(id.clone(), digest(value)?);
        if id == "goal" {
            authority.insert(
                "goal-source".into(),
                digest(&request.intent["goal_source"])?,
            );
        }
    }
    let mut blobs = BTreeMap::new();
    for citation in &r.citations {
        let document = request
            .documents
            .iter()
            .find(|d| d.id == citation.source)
            .ok_or("REVIEW_CACHE_SOURCE stage=verdict_cache")?;
        // Growing native transcripts are keyed by the exact cited whole unit,
        // rather than invalidated by an unrelated appended native record.
        let fingerprint = if matches!(document.kind.as_str(), "worker_transcript" | "ledger") {
            digest(&json!({"quote":citation.quote,"occurrence":citation.occurrence}))?
        } else {
            document.digest.clone()
        };
        let key = format!(
            "{}:{}",
            citation.source,
            digest(&json!({"quote":citation.quote,"occurrence":citation.occurrence}))?
        );
        blobs.insert(key, fingerprint);
    }
    let plans = super::dependencies::plans(request, r);
    let components = super::dependencies::components(request, r)?;
    let tests = super::dependencies::tests(request, r)?;
    Ok(Evidence {
        authority,
        blobs,
        plans,
        components,
        tests,
    })
}
pub(super) fn digest(value: &Value) -> Result<String> {
    Ok(format!(
        "sha256:{:x}",
        Sha256::digest(serde_json::to_vec(value)?)
    ))
}
fn snapshot(
    request: &Request,
    r: &decision_units::Requirement,
) -> Result<(Evidence, Option<String>)> {
    match fingerprint(request, r) {
        Ok(evidence) => Ok((evidence, None)),
        Err(error)
            if error.to_string().starts_with("REVIEW_CACHE_COMPONENT")
                || error.to_string().starts_with("REVIEW_CACHE_RECEIPT") =>
        {
            let mut without_component = r.clone();
            without_component.components.clear();
            without_component.checks.clear();
            Ok((
                fingerprint(request, &without_component)?,
                Some(error.to_string()),
            ))
        }
        Err(error) => Err(error),
    }
}
