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
    let mut coverage = serde_json::to_value(&outcome)?;
    super::super::light_commit::coverage(request, report, &mut coverage);
    let root = store.directory.join("verdict-cache");
    kah_core::run_store::ensure_private_directory(&root)?;
    for requirement in &units.requirements {
        let mut entry = entry(request, report, judgment, &outcome, requirement, head)?;
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
    let confirmed = !outcome.needs_thinker.contains(id)
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
        evidence: fingerprint(request, requirement)?,
        confirmed,
        always: always(
            requirement.dimension,
            unconditional,
            claim,
            matches!(requirement.judgment, Thinking::NeedsUser { .. })
                || requirement.existing_question_id.is_some(),
        ),
        verdict: json!({"requirement":requirement,"answers":answers,"protocol":report.decision_units.as_ref().unwrap().version()}),
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
    let plans = request
        .documents
        .iter()
        .filter(|d| d.kind == "plan")
        .map(|d| (d.uri.clone(), d.digest.clone()))
        .collect();
    let components = r
        .components
        .iter()
        .map(|c| Ok((c.name.clone(), digest(&serde_json::to_value(c)?)?)))
        .collect::<Result<_>>()?;
    let tests = r
        .checks
        .iter()
        .map(|c| Ok((c.name.clone(), digest(&serde_json::to_value(c)?)?)))
        .collect::<Result<_>>()?;
    Ok(Evidence {
        authority,
        blobs,
        plans,
        components,
        tests,
    })
}
fn digest(value: &Value) -> Result<String> {
    Ok(format!(
        "sha256:{:x}",
        Sha256::digest(serde_json::to_vec(value)?)
    ))
}
