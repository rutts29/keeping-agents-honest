use kah_cli::observer_review::{
    decisions,
    state::{Purpose, Request},
};
use kah_observer::{
    decision_units::ReviewUnits,
    judge::{Answer, Judgment, Metrics},
};
use serde_json::{Value, json};
use std::collections::BTreeMap;

fn request() -> Request {
    Request {
        id: "review-unit".into(),
        worker: "worker".into(),
        purpose: Purpose::Checkpoint,
        turn: "t".into(),
        head: Some("sha".into()),
        intent: json!({"goal":"Original words","items":[]}),
        trigger: json!({}),
        documents: vec![],
        created_ms: 0,
    }
}
fn units(kind: Value) -> ReviewUnits {
    // Applicability is validated independently.
    serde_json::from_value(json!({"requirements":[{"id":"integration","name":"Integration","dimension":"integration","authority_items":["goal"],"deferral_id":"d","citations":[],"components":[],"checks":[],"applicability":{"due":true,"unconditional":false,"reasoning":"This dependency closure is due at this checkpoint.","evidence_citations":[0]},"judgment":kind}],"proportion":null})).unwrap()
}
fn judgment(choice: &str) -> Judgment {
    Judgment {
        backend: "jev".into(),
        model: "jev-latest".into(),
        answers: BTreeMap::from([
            (
                "requirement.integration".into(),
                Answer::Choice {
                    choice: choice.into(),
                    confidence: 0.95,
                    probabilities: BTreeMap::from([(choice.into(), 0.95)]),
                },
            ),
            (
                "applicability.integration".into(),
                Answer::Choice {
                    choice: "supported".into(),
                    confidence: 0.95,
                    probabilities: BTreeMap::from([("supported".into(), 0.95)]),
                },
            ),
        ]),
        metrics: Metrics::default(),
        fallback_reason: None,
    }
}
#[test]
fn unresolved_question_is_pending_not_a_failed_checkpoint() {
    let units = units(
        json!({"kind":"needs_user","reason":"ambiguous_instructions","summary":"Two meanings remain.","question":"Which meaning?","options":["First","Second"],"evidence_citations":[0],"ledger_review":{"reasoning":"No existing answer.","citations":[0]}}),
    );
    let assessment = decisions::assessment(&request(), &units, &judgment("unclear"), vec![]);
    assert_eq!(assessment["state"], "checkpoint_needs_user");
    assert!(assessment["passed"].is_null());
    assert_eq!(assessment["needs_user"], json!(["integration"]));
}
#[test]
fn only_validated_satisfaction_closes_the_specific_tracked_dependency() {
    let units = units(
        json!({"kind":"semantic","decision":"satisfied","reasoning":"This exact requirement now passes.","evidence_citations":[0]}),
    );
    let deferrals = vec![json!({"id":"d"}), json!({"id":"unrelated"})];
    let accepted = decisions::assessment(
        &request(),
        &units,
        &judgment("supported"),
        deferrals.clone(),
    );
    assert_eq!(accepted["deferrals"], json!([{"id":"unrelated"}]));
    let doubtful = decisions::assessment(&request(), &units, &judgment("unclear"), deferrals);
    assert_eq!(doubtful["state"], "checkpoint_needs_thinker");
    assert_eq!(doubtful["deferrals"].as_array().unwrap().len(), 2);
}

#[test]
fn one_component_requirement_cannot_replace_full_checkpoint_coverage() {
    let mut units = units(
        json!({"kind":"semantic","decision":"satisfied","reasoning":"A component works.","evidence_citations":[0]}),
    );
    assert!(
        decisions::checkpoint_coverage(&request(), &units, &[json!({"id":"d"})])
            .unwrap_err()
            .to_string()
            .starts_with("REVIEW_REQUIREMENT_DIMENSION")
    );
    for dimension in [
        kah_observer::decision_units::Dimension::Intent,
        kah_observer::decision_units::Dimension::Drift,
    ] {
        let mut r = units.requirements[0].clone();
        r.id = format!("{dimension:?}");
        r.dimension = dimension;
        r.deferral_id = None;
        units.requirements.push(r);
    }
    decisions::checkpoint_coverage(&request(), &units, &[json!({"id":"d"})]).unwrap();
    assert!(
        decisions::checkpoint_coverage(&request(), &units, &[json!({"id":"other"})])
            .unwrap_err()
            .to_string()
            .starts_with("REVIEW_DEFERRAL_UNKNOWN")
    );
}
