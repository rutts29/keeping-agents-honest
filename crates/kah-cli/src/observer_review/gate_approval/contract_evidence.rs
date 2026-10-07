//! Record the particular whole source units considered for this item.
use super::followup::{Attempt, BuilderOutcome, VerificationRound};
use super::*;
use kah_observer::verification_contract::Item;

pub(super) fn attempts(
    request: &Request,
    report: &Report,
    item: &Item,
    round: &VerificationRound,
    assessment: &Value,
) -> Result<Vec<Attempt>> {
    super::super::failure::validation(super::super::failure::Origin::ReportValidation, || {
        report_attempts(request, report, item, round, assessment)
    })
}
fn report_attempts(
    request: &Request,
    report: &Report,
    item: &Item,
    round: &VerificationRound,
    assessment: &Value,
) -> Result<Vec<Attempt>> {
    let requirement = report
        .decision_units
        .as_ref()
        .and_then(|u| u.requirements.iter().find(|r| r.id == item.requirement_id))
        .ok_or("REVIEW_CONTRACT_ITEM stage=evidence")?;
    let mut tried = vec![];
    for index in &item.evidence_citations {
        let citation = requirement
            .citations
            .get(*index)
            .ok_or("REVIEW_CONTRACT_CITATION stage=evidence")?;
        let document = request
            .documents
            .iter()
            .find(|d| d.id == citation.source)
            .ok_or("REVIEW_CONTRACT_SOURCE stage=evidence")?;
        if citation.digest != document.digest || !document.text.contains(&citation.quote) {
            return Err(
                "REVIEW_SOURCE_CHANGED stage=evidence: item source is no longer its checked source"
                    .into(),
            );
        }
        tried.push(Attempt {
            what: format!(
                "Considered {} evidence from {} for {} (source digest {}).",
                document.kind, document.uri, item.claim, document.digest
            ),
            receipt: document.id.clone(),
        });
    }
    if assessment["needs_thinker"]
        .as_array()
        .is_some_and(|ids| ids.iter().any(|id| id == &item.requirement_id))
    {
        let reply = match &round
            .builder
            .as_ref()
            .ok_or("REVIEW_CONTRACT_BUILDER stage=evidence")?
            .response
        {
            BuilderOutcome::Evidence { response } => {
                format!("read the recorded native reply at {}", response.record)
            }
            BuilderOutcome::Unavailable { reason } => format!("no reply was available: {reason}"),
        };
        tried.push(Attempt {
            what: format!(
                "The shared evidence request named item {}; {}.",
                item.requirement_id, reply
            ),
            receipt: round.builder_receipt(),
        });
    }
    Ok(tried)
}

pub(super) fn selected(request: &Request, report: &Report, item: &Item) -> Result<Vec<Value>> {
    super::super::failure::validation(super::super::failure::Origin::ReportValidation, || {
        report_sources(request, report, item)
    })
}
fn report_sources(request: &Request, report: &Report, item: &Item) -> Result<Vec<Value>> {
    let requirement = report
        .decision_units
        .as_ref()
        .and_then(|u| u.requirements.iter().find(|r| r.id == item.requirement_id))
        .ok_or("REVIEW_CONTRACT_ITEM stage=evidence")?;
    item.evidence_citations.iter().map(|index| {
        let citation=requirement.citations.get(*index).ok_or("REVIEW_CONTRACT_CITATION stage=evidence")?;
        let document=request.documents.iter().find(|d|d.id==citation.source).ok_or("REVIEW_CONTRACT_SOURCE stage=evidence")?;
        if citation.digest!=document.digest || !document.text.contains(&citation.quote) {return Err("REVIEW_SOURCE_CHANGED stage=evidence".into());}
        Ok(json!({"source":document.id,"uri":document.uri,"kind":document.kind,"digest":document.digest,"quote":citation.quote,"occurrence":citation.occurrence}))
    }).collect()
}

#[cfg(test)]
#[path = "../../../tests/observer_unit/contract_evidence.rs"]
mod tests;
