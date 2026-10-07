//! Only the host verification boundary can publish controller fact evidence.
use crate::{
    Result,
    observer_review::{gate_approval, state::Request},
};
use kah_observer::review_evidence::Document;
use serde_json::{Value, json};

pub fn add(request: &mut Request) -> Result<()> {
    let Some(proof) = gate_approval::captured(request)? else {
        return Ok(());
    };
    let mut tags = vec![];
    for doc in request
        .documents
        .iter()
        .filter(|d| d.id == "git" && d.kind == "controller_facts")
    {
        let git: Value = serde_json::from_str(&doc.text)?;
        for line in git["tags"].as_str().unwrap_or("").lines() {
            let mut parts = line.split_whitespace();
            if let (Some(head), Some(reference), None) = (parts.next(), parts.next(), parts.next())
                && let Some(name) = reference.strip_prefix("refs/tags/")
            {
                tags.push(json!({"name":name,"head":head}));
            }
        }
    }
    let value = json!({"worker":request.worker,"request":request.id,"head":request.head,
        "fresh_checkout":proof.fresh_checkout,"tests":proof.tests,"tags":tags});
    request.documents.push(Document::new(
        "controller-receipts",
        "controller_verification",
        "kah:controller-receipts",
        &serde_json::to_string_pretty(&value)?,
    ));
    Ok(())
}
pub fn bind(request: &Request, report: &kah_observer::review_evidence::Report) -> Result<()> {
    for r in report.decision_units.iter().flat_map(|u| &u.requirements) {
        if let kah_observer::decision_units::Thinking::ControllerFact { fact, .. } = &r.judgment {
            if fact.worker != request.worker
                || fact.request != request.id
                || request.head.as_deref() != Some(&fact.head)
            {
                return Err("REVIEW_CONTROLLER_FACT_BINDING stage=report".into());
            }
            fact.verify(r, &request.documents)?;
        }
    }
    Ok(())
}
