//! Verify the optional v4 report against the complete protected authority set.
use super::state::Request;
use crate::Result;
use kah_observer::review_evidence::Report;
use serde_json::Value;
mod coverage;
mod scoped;
mod steering;
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
    if let Some(units) = &report.decision_units {
        super::questions::validate(request, units)?;
    }
    verify_evidence(request, report)
}
pub fn verify_evidence(request: &Request, report: &Report) -> Result<Option<Value>> {
    super::completion_due::bind(request, report)?;
    super::capture::controller_events::bind(request, report)?;
    super::capture::controller_receipts::bind(request, report)?;
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
    use kah_observer::decision_units;
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
    steering::context(request, report, &mut input)?;
    crate::privacy::scrub(&mut input);
    let mut questions = decision_units::questions(units);
    super::gate_approval::prepare_contracts(request, report, &mut input, &mut questions)?;
    if request.purpose == super::state::Purpose::Checkpoint
        && super::checkpoint_claim::doubtful(request)
    {
        questions.insert("claim_made".into(), super::checkpoint_claim::question());
    }
    if super::preparation::pending(&request.worker, &request.id)? {
        super::preparation::accept(&request.worker, &request.id)?;
    }
    let (judgment, prepared, archive) = if units.version() >= 8 {
        scoped::evaluate(home, request, &input, &questions)?
    } else {
        legacy(home, request, &input, &questions, units.version())?
    };
    match super::checkpoint_claim::assessment(&judgment) {
        Ok(Some(assessment)) => return Ok((judgment, prepared, assessment)),
        Err(error)
            if error
                .to_string()
                .starts_with("REVIEW_PHASE_CLAIM_UNCERTAIN") =>
        {
            if let Some(assessment) =
                super::checkpoint_claim::record_uncertainty(request, report, &judgment)?
            {
                return Ok((judgment, prepared, assessment));
            }
            return Err(error);
        }
        Err(error) => return Err(error),
        _ => (),
    }
    checkpoint_coverage(request, units, &deferrals)?;
    let mut assessment = assessment(request, units, &judgment, deferrals);
    steering::apply(request, units, &judgment, &mut assessment)?;
    assessment["private_replay"] = archive;
    assessment["contract_coverage"] =
        super::gate_approval::contract_outcome(request, report, &judgment)?;
    assessment["jev_called"] = (!questions.is_empty()).into();
    Ok((judgment, prepared, assessment))
}
fn legacy(
    home: &std::path::Path,
    request: &Request,
    input: &Value,
    questions: &kah_observer::judge::Questions,
    version: u32,
) -> Result<(kah_observer::judge::Judgment, Value, Value)> {
    use kah_observer::judge;
    let (judgment, prepared) = if questions.is_empty() {
        (
            judge::Judgment {
                backend: "not_requested".into(),
                model: "none".into(),
                answers: Default::default(),
                metrics: Default::default(),
                fallback_reason: None,
            },
            input.clone(),
        )
    } else {
        super::preparation::evaluate(home, request, input, questions)?
    };
    let archive = capture(home, request, &judgment, &prepared, questions, version)?;
    Ok((judgment, prepared, serde_json::to_value(archive)?))
}
pub(super) fn capture(
    home: &std::path::Path,
    request: &Request,
    judgment: &kah_observer::judge::Judgment,
    input: &Value,
    questions: &kah_observer::judge::Questions,
    version: u32,
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
        version,
    )?))
}

pub fn assessment(
    request: &Request,
    units: &kah_observer::decision_units::ReviewUnits,
    judgment: &kah_observer::judge::Judgment,
    deferrals: Vec<Value>,
) -> Value {
    use kah_observer::decision_units::{self, SemanticDecision, Thinking};
    let phase_due = super::completion_due::reviewed_phase(request, judgment);
    let outcome = decision_units::assess_with_phase_due(units, judgment, phase_due);
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
                && !outcome.needs_thinker.contains(&r.id)
                && !outcome.not_due.contains(&r.id)
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
    serde_json::json!({"state":format!("{purpose}_{state}"),"passed":if state=="passed" {Some(true)}else if state=="failed" {Some(false)}else{None},"violations":outcome.violations,"needs_user":outcome.needs_user,"needs_thinker":outcome.needs_thinker,"not_due":outcome.not_due,"deferred":outcome.deferred,"unproven":outcome.violations.iter().chain(&outcome.needs_user).chain(&outcome.needs_thinker).collect::<Vec<_>>(),"deferrals":open,"out_of_proportion":outcome.out_of_proportion,"checkpoint_question_set":units.version(),"controller_phase_due":if phase_due{Some(serde_json::json!({"worker":request.worker,"request":request.id,"head":request.head,"source":"persistent_verified_claim","judgment_key":"claim_made"}))}else{None}})
}
