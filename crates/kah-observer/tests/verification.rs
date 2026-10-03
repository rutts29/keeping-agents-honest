use kah_observer::{
    judge::{Answer, Judgment, Metrics},
    verification,
};
use std::collections::BTreeMap;
fn judgment(kind: &str, confidence: f64) -> Judgment {
    Judgment {
        backend: "jev".into(),
        model: "jev-latest".into(),
        answers: BTreeMap::from([
            ("is_steering".into(), Answer::Noul { noul: 0.95 }),
            (
                "steer_kind".into(),
                Answer::Choice {
                    choice: kind.into(),
                    confidence,
                    probabilities: BTreeMap::from([(kind.into(), 1.0)]),
                },
            ),
        ]),
        metrics: Metrics::default(),
        fallback_reason: None,
    }
}
#[test]
fn uncertain_goal_scope_and_supersession_wait_for_verification() {
    for kind in ["goal_change", "scope_expansion", "scope_reduction"] {
        assert!(verification::uncertain_impact(&judgment(kind, 0.6)));
        assert!(!verification::uncertain_impact(&judgment(kind, 0.99)));
    }
    assert!(!verification::uncertain_impact(&judgment(
        "new_constraint",
        0.6
    )));
    let mut replacement = judgment("correction", 0.99);
    replacement.answers.insert(
        "supersedes".into(),
        Answer::Choice {
            choice: "old-constraint".into(),
            confidence: 0.8,
            probabilities: BTreeMap::from([("old-constraint".into(), 0.8), ("none".into(), 0.2)]),
        },
    );
    assert!(verification::uncertain_impact(&replacement));
}
#[test]
fn low_confidence_tripwire_can_request_evidence_but_is_not_a_reminder() {
    let mut j = judgment("none", 0.99);
    j.answers.insert(
        "alignment".into(),
        Answer::Choice {
            choice: "accidental_drift".into(),
            confidence: 0.55,
            probabilities: BTreeMap::from([
                ("accidental_drift".into(), 0.6),
                ("on_track".into(), 0.4),
            ]),
        },
    );
    let actions = vec![serde_json::json!({"id":"tool-1"})];
    assert!(verification::trigger(&j, &actions).is_some());
    assert_ne!(
        kah_observer::judge::decision::assessment(&j, &actions, 0).state,
        kah_observer::judge::decision::AssessmentState::MaterialDrift
    );
}
#[test]
fn pending_verification_cannot_record_intent() {
    let pending = kah_observer::intake::Decision::Verification {
        judgment: Box::new(judgment("goal_change", 0.6)),
    };
    assert!(!pending.records_intent());
    assert!(kah_observer::intake::Decision::Force.records_intent());
    assert!(!kah_observer::intake::Decision::Aside.records_intent());
}

#[test]
fn uncertain_no_replacement_choice_still_checks_a_plausible_supersession() {
    let mut j = judgment("correction", 0.99);
    j.answers.insert(
        "supersedes".into(),
        Answer::Choice {
            choice: "none".into(),
            confidence: 0.1,
            probabilities: BTreeMap::from([("none".into(), 0.51), ("old-constraint".into(), 0.49)]),
        },
    );
    assert!(verification::uncertain_impact(&j));
    j.answers.insert(
        "supersedes".into(),
        Answer::Choice {
            choice: "none".into(),
            confidence: 0.99,
            probabilities: BTreeMap::from([("none".into(), 0.99), ("old-constraint".into(), 0.01)]),
        },
    );
    assert!(!verification::uncertain_impact(&j));
}
