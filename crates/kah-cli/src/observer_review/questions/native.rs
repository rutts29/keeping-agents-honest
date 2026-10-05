//! Preserve an already asked native question, rather than rephrasing it.
use super::*;
use kah_observer::{decision_units::Requirement, review_evidence::Report};

pub(super) fn verified(request: &Request, report: &Report) -> Result<Option<Value>> {
    if report
        .decision_units
        .as_ref()
        .is_some_and(|u| u.requirements.iter().any(|r| r.worker_question.is_some()))
    {
        return super::super::decisions::verify_evidence(request, report);
    }
    Ok(None)
}
pub(super) fn source(verified: &Option<Value>, requirement: &Requirement) -> Result<Option<Value>> {
    let Some(index) = requirement.worker_question else {
        return Ok(None);
    };
    let citation = verified
        .as_ref()
        .and_then(|v| v["requirements"].as_array())
        .and_then(|units| units.iter().find(|u| u["id"] == requirement.id))
        .and_then(|u| u["citations"].get(index))
        .ok_or("REVIEW_WORKER_QUESTION_CITATION stage=question")?;
    let mut pointer = citation.clone();
    pointer.as_object_mut().unwrap().remove("quote");
    Ok(Some(pointer))
}
pub(super) fn identity(
    worker: &str,
    r: &Requirement,
    question: &str,
    options: &[String],
    native: &Option<Value>,
) -> Value {
    if let Some(native) = native {
        let ids = native["native_records"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|r| r["record_id"].as_str())
            .collect::<Vec<_>>();
        return json!([worker, r.id, question, ids]);
    }
    json!([worker, r.id, r.authority_items, question, options])
}
pub(super) fn label(question: &Value) -> &'static str {
    if question["question_origin"] == "worker" {
        "Worker's question (verbatim):\n"
    } else {
        ""
    }
}
