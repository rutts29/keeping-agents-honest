use kah_observer::{
    decision_units::{self, ReviewUnits},
    judge::{Answer, Judgment, Metrics},
};
use serde_json::json;
use std::collections::BTreeMap;

fn units(kind: serde_json::Value) -> ReviewUnits {
    serde_json::from_value(json!({"requirements":[{"id":"current-contract","name":"Current contract","dimension":"intent","authority_items":["goal"],"citations":[],"components":[],"checks":[],"judgment":kind}],"proportion":null})).unwrap()
}
fn judgment(key: &str, choice: &str, p: f64, confidence: f64) -> Judgment {
    Judgment {
        backend: "jev".into(),
        model: "jev-latest".into(),
        answers: BTreeMap::from([(
            key.into(),
            Answer::Choice {
                choice: choice.into(),
                confidence,
                probabilities: BTreeMap::from([(choice.into(), p)]),
            },
        )]),
        metrics: Metrics::default(),
        fallback_reason: None,
    }
}
#[test]
fn semantic_decision_needs_jev_validation_not_model_assurance() {
    let semantic = units(
        json!({"kind":"semantic","decision":"satisfied","reasoning":"Equivalent behavior meets the exact quoted requirement.","evidence_citations":[0]}),
    );
    let outcome = decision_units::assess(
        &semantic,
        &judgment("requirement.current-contract", "supported", 0.95, 0.95),
    );
    assert!(outcome.passed);
    assert!(
        !decision_units::assess(
            &semantic,
            &judgment("requirement.current-contract", "supported", 0.89, 0.95)
        )
        .passed
    );
    assert!(
        !decision_units::assess(
            &semantic,
            &judgment("requirement.current-contract", "supported", 0.95, 0.84)
        )
        .passed
    );
    let missing = judgment("other", "supported", 0.95, 0.95);
    assert_eq!(
        decision_units::assess(&semantic, &missing).needs_thinker,
        ["current-contract"]
    );
}
#[test]
fn confirmed_violation_blocks_but_uncertainty_is_a_named_user_decision() {
    let violation = units(
        json!({"kind":"semantic","decision":"violated","reasoning":"The user prohibited publication; the tool published.","evidence_citations":[0]}),
    );
    let outcome = decision_units::assess(
        &violation,
        &judgment("requirement.current-contract", "supported", 0.95, 0.95),
    );
    assert_eq!(outcome.violations, ["current-contract"]);
    assert!(outcome.needs_user.is_empty());
    let factual = units(
        json!({"kind":"fact","condition":"The observed native command returned success.","evidence_citations":[0]}),
    );
    assert_eq!(
        decision_units::assess(
            &factual,
            &judgment("requirement.current-contract", "unclear", 0.95, 0.95)
        )
        .needs_thinker,
        ["current-contract"]
    );
}
#[test]
fn unresolved_thinker_question_does_not_invent_a_jev_verdict() {
    let uncertain = units(
        json!({"kind":"needs_user","reason":"complex_context","summary":"Two authorization sources cannot be reconciled.","question":"Which contract should apply?","options":["First contract","Second contract"],"evidence_citations":[0]}),
    );
    assert!(decision_units::questions(&uncertain).is_empty());
    assert!(
        !decision_units::assess(&uncertain, &judgment("other", "supported", 0.95, 0.95)).passed
    );
}

#[test]
fn literal_question_paths_keep_every_complete_verified_requirement() {
    let source = json!({"requirements":[{"id":"first","authority_items":["goal"],"citations":[{"quote":"Whole original words."}]},{"id":"second","judgment":{"kind":"needs_user","question":"Which one?"}}],"proportion":{"worst_case_consequence":"A local retry"}});
    let named = ReviewUnits::named_state(&source).unwrap();
    assert_eq!(named["requirements"]["first"], source["requirements"][0]);
    assert_eq!(named["requirements"]["second"], source["requirements"][1]);
    assert_eq!(named["proportion"], source["proportion"]);
}
