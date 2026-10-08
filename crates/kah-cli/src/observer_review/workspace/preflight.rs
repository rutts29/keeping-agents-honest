//! Native submissions use captured controller records; host state is not mounted.
//! The consumer independently revalidates live references before judging/applying.
use super::*;
use kah_observer::decision_units::Thinking;
use serde_json::Value;

pub(super) fn validate(request: &Request, report: &Report, findings: Vec<Value>) -> Result<()> {
    super::super::checkpoint_claim::require_evidence(request, &findings)?;
    super::super::decisions::verify_evidence(request, report)?;
    let Some(units) = &report.decision_units else {
        return Ok(());
    };
    let actions = findings
        .iter()
        .map(|v| json!({"evidence":v}))
        .collect::<Vec<_>>();
    super::super::decisions::preflight_contexts(request, report, &actions)?;
    let questions: Vec<Value> = request
        .documents
        .iter()
        .find(|d| d.id == "pending-questions" && d.kind == "controller_questions")
        .map(|d| serde_json::from_str(&d.text))
        .transpose()?
        .unwrap_or_default();
    for r in &units.requirements {
        if let Some(id) = &r.existing_question_id {
            let q = questions.iter().find(|q| q["id"] == *id && q["status"] == "pending")
                .ok_or("REVIEW_QUESTION_REFERENCE stage=question: unknown or resolved captured reference")?;
            if !super::super::questions::question_scope(q, request, &r.id) {
                return Err(
                    "REVIEW_QUESTION_REFERENCE_SCOPE stage=question: different worker or decision"
                        .into(),
                );
            }
        } else if matches!(r.judgment, Thinking::NeedsUser { .. })
            && let Some(q) = questions.iter().find(|q| {
                q["status"] == "pending"
                    && super::super::questions::question_scope(q, request, &r.id)
            })
        {
            return Err(format!(
                "REVIEW_QUESTION_REFERENCE_REQUIRED stage=question: use existing_question_id={}",
                q["id"]
            )
            .into());
        }
    }
    if !super::super::checkpoint_claim::doubtful(request) {
        let actions = findings
            .into_iter()
            .map(|v| json!({"evidence":v}))
            .collect::<Vec<_>>();
        let deferrals = super::super::deferrals::collect(request, &actions)?;
        let effective = super::super::verdict_cache::integration::units(request, report)?;
        super::super::decisions::checkpoint_coverage(request, &effective, &deferrals)?;
    }
    Ok(())
}
