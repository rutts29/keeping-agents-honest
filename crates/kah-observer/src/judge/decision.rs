use super::*;
pub fn complete(judgment: &Judgment, outcomes: &[(String, String)]) -> bool {
    judgment.backend == "jev" && !outcomes.is_empty()
        && outcomes.iter().all(|(id,_)| matches!(judgment.answers.get(&format!("outcome_met.{id}")),Some(Answer::Choice{choice,confidence,probabilities}) if choice=="satisfied" && *confidence>=questions::COMPLETE_CONFIDENCE && probabilities.get("satisfied").is_some_and(|p|*p>=questions::COMPLETE_PROBABILITY)))
}
