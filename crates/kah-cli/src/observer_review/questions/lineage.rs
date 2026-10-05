//! A checked question reference refines one wait; it never creates a second.
use super::*;
use kah_observer::decision_units::{Requirement, ReviewUnits};

pub fn validate(request: &Request, units: &ReviewUnits) -> Result<()> {
    let root = super::super::state::Store::directory(&request.worker)?;
    for r in &units.requirements {
        target(&root, request, r)?;
        if units.version() == 8
            && matches!(r.judgment, Thinking::NeedsUser { .. })
            && r.existing_question_id.is_none()
        {
            require_new(&root, request, r)?;
        }
    }
    Ok(())
}

pub(super) fn target(root: &Path, request: &Request, r: &Requirement) -> Result<Option<Value>> {
    let Some(id) = &r.existing_question_id else {
        return Ok(None);
    };
    let questions = pending(root)?;
    let question = questions.into_iter().find(|q| q["id"] == *id).ok_or(
        "REVIEW_QUESTION_REFERENCE stage=question: reference must name an active question",
    )?;
    if question["worker"] != request.worker || question["requirement"] != r.id {
        return Err(
            "REVIEW_QUESTION_REFERENCE_SCOPE stage=question: different worker or decision".into(),
        );
    }
    Ok(Some(question))
}

pub(super) fn require_new(root: &Path, request: &Request, r: &Requirement) -> Result<()> {
    if pending(root)?
        .iter()
        .any(|q| q["worker"] == request.worker && q["requirement"] == r.id)
    {
        return Err("REVIEW_QUESTION_REFERENCE_REQUIRED stage=question: cite existing_question_id; one decision has one active wait".into());
    }
    Ok(())
}

pub(super) fn refine(previous: &Value, proposed: &Value) -> Result<Option<Value>> {
    if previous["question"] == proposed["question"]
        && previous["question_origin"] == proposed["question_origin"]
    {
        return Ok(None);
    }
    if proposed["question_origin"] != "worker" {
        return Err("REVIEW_QUESTION_REFINEMENT_NATIVE stage=question: refine with the worker's exact native quote".into());
    }
    let mut updated = proposed.clone();
    for field in ["id", "authority_at_question", "goal_at_question", "at_ms"] {
        updated[field] = previous[field].clone();
    }
    let mut prior = previous.clone();
    prior.as_object_mut().unwrap().remove("lineage");
    prior["status"] = "superseded".into();
    prior["superseded_by_request"] = proposed["request"].clone();
    let mut lineage = previous["lineage"].as_array().cloned().unwrap_or_default();
    lineage.push(prior);
    updated["lineage"] = json!(lineage);
    updated["refined_at_ms"] = proposed["at_ms"].clone();
    updated["delivery"] = "queued".into();
    Ok(Some(updated))
}
