//! Verify the optional report against the complete protected authority set.
use super::state::Request;
use crate::Result;
use kah_observer::review_evidence::Report;
use serde_json::Value;
#[cfg(test)]
#[path = "../../tests/observer_unit/report_boundaries.rs"]
mod boundary_tests;
mod coverage;
pub(crate) mod scoped;
mod scoped_batch;
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
    super::failure::validation(super::failure::Origin::ReportValidation, || {
        verify_sources(request, report)
    })
}
fn verify_sources(request: &Request, report: &Report) -> Result<Option<Value>> {
    super::completion_due::bind(request, report)?;
    super::capture::controller_events::bind(request, report)?;
    super::capture::controller_receipts::bind(request, report)?;
    super::verdict_cache::integration::verify(request, report)
}

/// Validate every independent source selection before acknowledging a report.
/// This does not judge it, access credentials, or execute its proposed checks.
pub(crate) fn preflight_contexts(
    request: &Request,
    report: &Report,
    actions: &[Value],
) -> Result<()> {
    super::failure::validation(super::failure::Origin::ReportValidation, || {
        let Some(units) = &report.decision_units else {
            return Ok(());
        };
        let verified =
            verify_evidence(request, report)?.ok_or("REVIEW_UNITS_MISSING stage=decision")?;
        let deferrals = super::deferrals::collect(request, actions)?;
        let mut input = super::checkpoint::decision_context(
            &request.intent,
            &request.trigger,
            request.head.as_deref(),
            &serde_json::json!(actions),
            serde_json::json!(deferrals),
        );
        input["decision"] = kah_observer::decision_units::ReviewUnits::named_state(&verified)?;
        steering::context(request, report, &mut input)?;
        let mut questions = kah_observer::decision_units::questions(units);
        if !super::commit_packet::is_commit(request) {
            super::gate_approval::prepare_contracts(request, report, &mut input, &mut questions)?;
        }
        if units.version() >= 8 {
            for key in questions.keys() {
                scoped::report_context(&input, key)?;
            }
        }
        Ok(())
    })
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
    let mut input = evaluation_input(request, report, actions, &deferrals, &verified)?;
    let mut questions = decision_units::questions(units);
    if !super::commit_packet::is_commit(request) {
        super::gate_approval::prepare_contracts(request, report, &mut input, &mut questions)?;
    }
    if request.purpose == super::state::Purpose::Checkpoint
        && super::checkpoint_claim::doubtful(request)
    {
        questions.insert("claim_made".into(), super::checkpoint_claim::question());
    }
    if super::preparation::pending(&request.worker, &request.id)? {
        super::preparation::accept(&request.worker, &request.id)?;
    }
    let (mut judgment, prepared, archive) = if units.version() >= 8 {
        scoped::evaluate(home, request, &input, &questions)?
    } else {
        legacy(home, request, &input, &questions, units.version())?
    };
    if super::commit_packet::is_commit(request) {
        let effective = super::verdict_cache::integration::units(request, report)?;
        let mut carried = serde_json::json!({});
        super::verdict_cache::integration::merge(request, report, &mut judgment, &mut carried)?;
        let mut result = super::light_commit::conclusion(
            &assessment(request, &effective, &judgment, deferrals),
            archive,
            !questions.is_empty(),
        );
        super::light_commit::coverage(request, report, &mut result);
        result["carried"] = carried["carried"].clone();
        return Ok((judgment, prepared, result));
    }
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
    serde_json::json!({"state":format!("{purpose}_{state}"),"passed":if state=="passed" {Some(true)}else if state=="failed" {Some(false)}else{None},"violations":outcome.violations,"needs_user":outcome.needs_user,"needs_thinker":outcome.needs_thinker,"not_due":outcome.not_due,"resolved_non_executable":outcome.resolved_non_executable,"resolved_preservation":outcome.resolved_preservation,"deferred":outcome.deferred,"unproven":outcome.violations.iter().chain(&outcome.needs_user).chain(&outcome.needs_thinker).collect::<Vec<_>>(),"deferrals":open,"out_of_proportion":outcome.out_of_proportion,"checkpoint_question_set":units.version(),"controller_phase_due":if phase_due{Some(serde_json::json!({"worker":request.worker,"request":request.id,"head":request.head,"source":"persistent_verified_claim","judgment_key":"claim_made"}))}else{None}})
}

fn evaluation_input(
    request: &Request,
    report: &Report,
    actions: &[Value],
    deferrals: &[Value],
    verified: &Value,
) -> Result<Value> {
    let mut input = super::checkpoint::decision_context(
        &request.intent,
        &request.trigger,
        request.head.as_deref(),
        &serde_json::json!(actions),
        serde_json::json!(deferrals),
    );
    input["decision"] = kah_observer::decision_units::ReviewUnits::named_state(verified)?;
    steering::context(request, report, &mut input)?;
    crate::privacy::scrub(&mut input);
    Ok(input)
}
