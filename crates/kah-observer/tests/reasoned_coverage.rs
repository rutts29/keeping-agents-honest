use kah_observer::{judge::Judgment, verification_contract::reasoned};
use serde_json::Value;

#[test]
fn recorded_positive_coverage_confirms_and_the_irrelevant_passing_check_never_does() {
    let evidence: Value = serde_json::from_str(include_str!(
        "../../../docs/gates/evidence/phase-5o-coverage-decision-replay/replay.json"
    ))
    .unwrap();
    for outcome in evidence["outcomes"].as_array().unwrap() {
        let mut judgment: Judgment = serde_json::from_value(outcome["judgment"].clone()).unwrap();
        let (_, answer) = judgment.answers.pop_first().unwrap();
        judgment
            .answers
            .insert("coverage_decision.item".into(), answer);
        assert_eq!(
            reasoned::covered(&judgment, "item"),
            outcome["id"] != "trivial-passing-command"
        );
    }
}
