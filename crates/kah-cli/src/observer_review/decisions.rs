//! Verify the optional v4 report against the complete protected authority set.
use super::state::Request;
use crate::Result;
use kah_observer::review_evidence::Report;
use serde_json::Value;
mod coverage;
pub use coverage::checkpoint as checkpoint_coverage;

pub fn active_items(intent: &Value) -> Vec<String> {
    let mut items = Vec::new();
    if !intent["goal"].is_null() {
        items.push("goal".into());
    }
    if let Some(authority) = intent["items"].as_array() {
        items.extend(
            authority
                .iter()
                .filter_map(|item| item["id"].as_str().map(str::to_owned)),
        );
    }
    items
}
pub fn verify(request: &Request, report: &Report) -> Result<Option<Value>> {
    report
        .verify_units(&request.documents, &active_items(&request.intent))
        .map_err(Into::into)
}

pub fn evaluate(
    home: &std::path::Path,
    request: &Request,
    report: &Report,
    actions: &[Value],
) -> Result<(kah_observer::judge::Judgment, Value, Value)> {
    use kah_observer::{decision_units, judge};
    let units = report.decision_units.as_ref().ok_or("REVIEW_UNITS_MISSING stage=decision: return named factual or reasoned semantic requirements")?;
    let verified = verify(request, report)?.ok_or("REVIEW_UNITS_MISSING stage=decision")?;
    let deferrals = super::deferrals::collect(request, actions)?;
    let mut input = super::checkpoint::decision_context(
        &request.intent,
        &request.trigger,
        request.head.as_deref(),
        &serde_json::json!(actions),
        serde_json::json!(deferrals),
    );
    input["decision"] = decision_units::ReviewUnits::named_state(&verified)?;
    crate::privacy::scrub(&mut input);
    let mut questions = decision_units::questions(units);
    if request.purpose == super::state::Purpose::Checkpoint
        && super::checkpoint_claim::doubtful(request)
    {
        questions.insert("claim_made".into(), super::checkpoint_claim::question());
    }
    if super::preparation::pending(&request.worker, &request.id)? {
        super::preparation::accept(&request.worker, &request.id)?;
    }
    let (judgment, prepared) = if questions.is_empty() {
        (
            judge::Judgment {
                backend: "not_requested".into(),
                model: "none".into(),
                answers: Default::default(),
                metrics: Default::default(),
                fallback_reason: None,
            },
            input,
        )
    } else {
        super::preparation::evaluate(home, request, &input, &questions)?
    };
    let archive = capture(home, request, &judgment, &prepared, &questions)?;
    if let Some(assessment) = super::checkpoint_claim::assessment(&judgment)? {
        return Ok((judgment, prepared, assessment));
    }
    checkpoint_coverage(request, units, &deferrals)?;
    let mut assessment = assessment(request, units, &judgment, deferrals);
    assessment["private_replay"] = serde_json::to_value(archive)?;
    assessment["jev_called"] = (!questions.is_empty()).into();
    Ok((judgment, prepared, assessment))
}
fn capture(
    home: &std::path::Path,
    request: &Request,
    judgment: &kah_observer::judge::Judgment,
    input: &Value,
    questions: &kah_observer::judge::Questions,
) -> Result<Option<std::path::PathBuf>> {
    if questions.is_empty() {
        return Ok(None);
    }
    let captured = crate::completion_capture::Request::new(&judgment.model, input, questions)?;
    Ok(Some(crate::completion_capture::save_versioned(
        home,
        &request.worker,
        request.head.as_deref().unwrap_or("uncommitted"),
        "named_decision",
        &captured,
        &Ok(judgment.clone()),
        kah_observer::decision_units::VERSION,
    )?))
}

pub fn assessment(
    request: &Request,
    units: &kah_observer::decision_units::ReviewUnits,
    judgment: &kah_observer::judge::Judgment,
    deferrals: Vec<Value>,
) -> Value {
    use kah_observer::decision_units::{self, SemanticDecision, Thinking};
    let outcome = decision_units::assess(units, judgment);
    let mut open = Vec::new();
    for item in deferrals {
        let closed = units.requirements.iter().any(|r| {
            r.deferral_id.as_deref() == item["id"].as_str()
                && matches!(
                    r.judgment,
                    Thinking::Semantic {
                        decision: SemanticDecision::Satisfied,
                        ..
                    }
                )
                && decision_units::confirmed(
                    judgment,
                    &format!("requirement.{}", r.id),
                    "supported",
                )
        });
        if !closed {
            open.push(item);
        }
    }
    let state = if !outcome.violations.is_empty() {
        "failed"
    } else if !outcome.needs_user.is_empty() {
        "needs_user"
    } else if !outcome.needs_thinker.is_empty() {
        "needs_thinker"
    } else {
        "passed"
    };
    let purpose = match request.purpose {
        super::state::Purpose::Checkpoint => "checkpoint",
        super::state::Purpose::Drift => "drift",
        super::state::Purpose::Steering => "steering",
        _ => "decision",
    };
    serde_json::json!({"state":format!("{purpose}_{state}"),"passed":if state=="passed" {Some(true)}else if state=="failed" {Some(false)}else{None},"violations":outcome.violations,"needs_user":outcome.needs_user,"needs_thinker":outcome.needs_thinker,"unproven":outcome.violations.iter().chain(&outcome.needs_user).chain(&outcome.needs_thinker).collect::<Vec<_>>(),"deferrals":open,"out_of_proportion":outcome.out_of_proportion,"checkpoint_question_set":decision_units::VERSION})
}
