use super::*;
use crate::semantic_resolver::{Assessment, AssessmentState};

pub fn assessment(judgment: &Judgment, actions: &[Value], failed_approaches: usize) -> Assessment {
    let alignment = judgment.answers.get("alignment");
    let material = matches!(alignment,Some(Answer::Choice{choice,confidence,..}) if choice=="accidental_drift" && *confidence>=questions::REMINDER_CONFIDENCE)
        && matches!(judgment.answers.get("drift_severity"),Some(Answer::Score{score,confidence,..}) if *score>=questions::MATERIAL_DISTANCE && *confidence>=questions::REMINDER_CONFIDENCE);
    let repeating = failed_approaches >= 2
        && matches!(judgment.answers.get("repeating_failed_approach"),Some(Answer::Noul{noul}) if *noul>=questions::STRONG_FAILURE_SIGNAL);
    let state = if material || repeating {
        AssessmentState::MaterialDrift
    } else if matches!(alignment,Some(Answer::Choice{choice,confidence,..}) if matches!(choice.as_str(),"on_track"|"user_authorized_scope_change") && *confidence>=questions::REMINDER_CONFIDENCE)
    {
        AssessmentState::Aligned
    } else {
        AssessmentState::Uncertain
    };
    let reason = if repeating {
        "Recent actions repeat a documented failed approach without a relevant correction."
    } else if material {
        "Recent native actions materially deviate from the current user-sourced intent. Compare the actions with the active requirements and correct the deviation."
    } else {
        "No confident material deviation was established by the typed questions."
    };
    Assessment {
        state,
        reason: reason.into(),
        action_ids: actions
            .iter()
            .filter_map(|v| v["id"].as_str().map(str::to_owned))
            .collect(),
    }
}

pub fn complete(judgment: &Judgment, outcomes: &[(String, String)]) -> bool {
    !outcomes.is_empty() && outcomes.len()<=24
        && outcomes.iter().all(|(id,_)| matches!(judgment.answers.get(&format!("outcome_met.{id}")),Some(Answer::Noul{noul}) if *noul>=questions::COMPLETE_PROBABILITY))
        && matches!(judgment.answers.get("completion"),Some(Answer::Choice{choice,confidence,..}) if choice=="complete" && *confidence>=questions::COMPLETE_CONFIDENCE)
}
