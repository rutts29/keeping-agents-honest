use kah_observer::{
    decision_units::{self, ReviewUnits},
    judge::{Answer, Judgment, Metrics},
    review_evidence::Document,
};
use serde_json::json;
use std::collections::BTreeMap;

fn units(due: bool, unconditional: bool) -> ReviewUnits {
    serde_json::from_value(json!({"requirements":[{
        "id":"receipt","name":"Post-commit receipt","dimension":"drift","authority_items":["goal"],
        "citations":[],"components":[],"checks":[],
        "applicability":{"due":due,"unconditional":unconditional,"reasoning":"The worker has not claimed completion yet.","evidence_citations":[0]},
        "judgment":{"kind":"semantic","decision":"violated","reasoning":"The receipt has not been produced yet.","evidence_citations":[0]}
    }]})).unwrap()
}
fn judgment(app: &str) -> Judgment {
    let answer = |choice: &str| Answer::Choice {
        choice: choice.into(),
        confidence: 0.95,
        probabilities: BTreeMap::from([(choice.into(), 0.95)]),
    };
    Judgment {
        backend: "jev".into(),
        model: "jev-latest".into(),
        answers: BTreeMap::from([
            ("applicability.receipt".into(), answer(app)),
            ("requirement.receipt".into(), answer("supported")),
        ]),
        metrics: Metrics::default(),
        fallback_reason: None,
    }
}
#[test]
fn unfinished_completion_obligation_is_not_a_violation_before_it_is_due() {
    let outcome = decision_units::assess(&units(false, false), &judgment("supported"));
    assert!(outcome.violations.is_empty());
    assert!(outcome.passed);
    assert_eq!(outcome.not_due, ["receipt"]);
}
#[test]
fn violation_requires_applicability_and_violation_to_both_validate() {
    assert_eq!(
        decision_units::assess(&units(true, false), &judgment("supported")).violations,
        ["receipt"]
    );
    let uncertain = decision_units::assess(&units(true, false), &judgment("unclear"));
    assert!(uncertain.violations.is_empty());
    assert_eq!(uncertain.needs_thinker, ["receipt"]);
}
#[test]
fn an_unconditional_prohibition_cannot_be_declared_not_due() {
    let doc = Document::new("ledger", "ledger", "kah:ledger", "Never commit notes.");
    let mut units = units(false, true);
    units.requirements[0].citations = vec![kah_observer::review_evidence::Citation {
        source: doc.id.clone(),
        digest: doc.digest.clone(),
        quote: doc.text.clone(),
        occurrence: 0,
    }];
    assert!(
        units
            .verify(&[doc], &["goal".into()])
            .unwrap_err()
            .starts_with("REVIEW_APPLICABILITY_UNCONDITIONAL")
    );
}

#[test]
fn an_old_report_without_applicability_cannot_authorize_a_new_violation() {
    let mut old = units(true, false);
    old.requirements[0].applicability = None;
    assert_eq!(old.version(), 5);
    let result = decision_units::assess(&old, &judgment("supported"));
    assert!(result.violations.is_empty());
    assert_eq!(result.needs_thinker, ["receipt"]);
}

#[test]
fn unconditional_prohibitions_always_apply_without_an_inferred_due_score() {
    let u = units(true, true);
    assert!(!decision_units::questions(&u).contains_key("applicability.receipt"));
    assert_eq!(
        decision_units::assess(&u, &judgment("unclear")).violations,
        ["receipt"]
    );
}
