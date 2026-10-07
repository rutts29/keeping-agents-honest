use super::super::followup::{Attempt, Binding};
use super::*;
fn result(id: &str, verified: bool, needs_check: bool) -> ItemResult {
    ItemResult {
        binding: Binding {
            worker: "worker".into(),
            review: "review-contract".into(),
            head: "523af6fcbf1990e2764bf7f28ced10c59d0d597e".into(),
            intent_revision: 2,
        },
        requirement_id: id.into(),
        claim: format!("Exact original contract {id}"),
        plan_digest: "fixture-plan".into(),
        verified,
        needs_check,
        semantic_risk_admissible: !verified && !needs_check,
        coverage: json!({"covered":verified}),
        check: None,
        tried: vec![Attempt {
            what: format!("Checked the source and receipt specific to {id}."),
            receipt: format!("{id}-receipt"),
        }],
    }
}
#[test]
fn exact_eleven_item_case_leaves_only_semantic_candidates_not_verification_work() {
    let units: kah_observer::decision_units::ReviewUnits = serde_json::from_str(include_str!(
        "../../../kah-observer/tests/fixtures/v9-eleven-unverified-units.json"
    ))
    .unwrap();
    let ids = units
        .requirements
        .iter()
        .map(|r| r.id.clone())
        .collect::<Vec<_>>();
    assert_eq!(ids.len(), 11);
    let results = ids
        .iter()
        .map(|id| {
            result(
                id,
                !matches!(id.as_str(), "checkpoint_intent" | "checkpoint_drift"),
                false,
            )
        })
        .collect::<Vec<_>>();
    let mut assessment = json!({"passed":null,"violations":[],"needs_user":[],"needs_thinker":ids});
    apply_items(&results, &mut assessment).unwrap();
    assert_eq!(
        assessment["needs_thinker"],
        json!(["checkpoint_intent", "checkpoint_drift"])
    );
    assert_eq!(assessment["contract_verified"].as_array().unwrap().len(), 9);
    assert!(assessment["needs_check"].as_array().unwrap().is_empty());
}
#[test]
fn uncovered_executable_item_sends_work_back_without_question_or_stalled_round() {
    let mut assessment = json!({"passed":null,"violations":[],"needs_user":[],"needs_thinker":["normalize_contract"]});
    apply_items(
        &[result("normalize_contract", false, true)],
        &mut assessment,
    )
    .unwrap();
    assert_eq!(assessment["passed"], false);
    assert_eq!(assessment["state"], "checkpoint_needs_check");
    assert!(assessment["needs_user"].as_array().unwrap().is_empty());
    assert_eq!(assessment["needs_check"][0]["id"], "normalize_contract");
}
#[test]
fn covered_all_means_no_user_question_but_violations_still_win() {
    let results = [result("normalize_contract", true, false)];
    let mut assessment = json!({"passed":null,"violations":[],"needs_user":[],"needs_thinker":["normalize_contract"]});
    apply_items(&results, &mut assessment).unwrap();
    assert_eq!(assessment["passed"], true);
    assessment = json!({"passed":false,"violations":["scope"],"needs_user":[],"needs_thinker":["normalize_contract"]});
    apply_items(&results, &mut assessment).unwrap();
    assert_eq!(assessment["passed"], false);
}
