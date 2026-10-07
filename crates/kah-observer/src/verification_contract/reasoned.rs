//! A chosen executable method asserts coverage; NeedsCheck declines it.
//! Reuse the semantic validator verbatim, never ask Jev to infer code coverage.
use super::*;
use crate::decision_units::{self, Requirement, ReviewUnits, SemanticDecision, Thinking};

pub fn prepare(
    item: &Item,
    requirement: &Requirement,
    checked: &Value,
    input: &Value,
    command: &str,
    receipt: &Value,
) -> Result<(Value, Questions), String> {
    let head = input["event"]["head"]
        .as_str()
        .ok_or("REVIEW_CONTRACT_HEAD stage=coverage")?;
    if receipt["commit"] != head
        || receipt["command"] != command
        || receipt["exit_code"] != 0
        || receipt["success"] != true
    {
        return Err("REVIEW_CONTRACT_RECEIPT_BINDING stage=coverage".into());
    }
    let mut r = requirement.clone();
    r.applicability = None;
    r.name = format!(
        "Executed command {command} on {head} covers requirement: {}",
        requirement.name
    );
    r.judgment = Thinking::Semantic {
        decision: SemanticDecision::Satisfied,
        reasoning: item.coverage_reasoning.clone(),
        evidence_citations: item.evidence_citations.clone(),
    };
    let mut local = serde_json::to_value(&r).map_err(|_| "REVIEW_CONTRACT_JSON stage=coverage")?;
    local["citations"] = checked["citations"].clone();
    let mut source = input.clone();
    source["decision"] = json!({"requirements":{&r.id:local}});
    let mut state = decision_units::context::decision(&source, &format!("requirement.{}", r.id))?;
    state["event"]["selected_host_check"] = receipt.clone();
    let units = ReviewUnits {
        version: Some(11),
        requirements: vec![r],
        proportion: None,
        steering: None,
    };
    let question = decision_units::questions(&units)
        .remove(&format!("requirement.{}", requirement.id))
        .ok_or("REVIEW_CONTRACT_QUESTION stage=coverage")?;
    Ok((
        state,
        Questions::from([("coverage_decision".into(), question)]),
    ))
}

pub fn covered(judgment: &Judgment, id: &str) -> bool {
    confirmed(judgment, &format!("coverage_decision.{id}"), "supported")
}
