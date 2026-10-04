use kah_observer::{
    checkpoint,
    judge::{Answer, Judgment, Metrics},
};
use std::collections::BTreeMap;

fn evidence(yes: &[&str]) -> Judgment {
    Judgment {
        backend: "jev".into(),
        model: "jev-latest".into(),
        answers: yes
            .iter()
            .map(|id| {
                (
                    id.to_string(),
                    Answer::Choice {
                        choice: "yes".into(),
                        confidence: 0.95,
                        probabilities: BTreeMap::from([
                            ("yes".into(), 0.95),
                            ("no".into(), 0.03),
                            ("unclear".into(), 0.02),
                        ]),
                    },
                )
            })
            .collect(),
        metrics: Metrics::default(),
        fallback_reason: None,
    }
}
#[test]
fn integration_is_composed_from_direct_execution_and_dependency_answers() {
    let current = evidence(&["intent", "drift", "work_accounted", "end_to_end"]);
    assert!(checkpoint::decide(&current).passed);
    let future = evidence(&[
        "intent",
        "drift",
        "work_accounted",
        "available_tests",
        "only_declared_missing",
        "deferral.d.absent",
        "deferral.d.blocks",
        "deferral.d.authorized_later",
    ]);
    assert!(checkpoint::decide_with_deferrals(&future, &["d"]).passed);
    assert!(!checkpoint::decide_with_deferrals(&future, &[]).passed);
    for required in [
        "deferral.d.absent",
        "deferral.d.blocks",
        "deferral.d.authorized_later",
        "available_tests",
        "only_declared_missing",
    ] {
        let mut incomplete = future.clone();
        incomplete.answers.remove(required);
        assert!(!checkpoint::decide_with_deferrals(&incomplete, &["d"]).passed);
    }
}
#[test]
fn repaired_work_closes_deferrals_only_on_independent_current_evidence() {
    let mut repaired = evidence(&[
        "intent",
        "drift",
        "work_accounted",
        "end_to_end",
        "deferral.d.closed",
    ]);
    assert!(checkpoint::decide_with_deferrals(&repaired, &["d"]).passed);
    repaired.answers.remove("deferral.d.closed");
    assert!(!checkpoint::decide_with_deferrals(&repaired, &["d"]).passed);
}
#[test]
fn question_set_contains_independent_conditions_without_conditional_overrides() {
    let questions = checkpoint::questions();
    for id in [
        "intent",
        "drift",
        "work_accounted",
        "end_to_end",
        "available_tests",
        "only_declared_missing",
    ] {
        assert!(questions.contains_key(id), "missing atomic question {id}");
    }
    assert_eq!(checkpoint::deferral_questions("d").len(), 4);
}
