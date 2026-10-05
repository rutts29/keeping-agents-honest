use kah_cli::observer_review::preparation::{Job, matches_decision};
use kah_observer::judge::{Question, Questions};
use serde_json::json;
#[test]
fn a_corrected_report_or_question_cannot_reuse_the_prior_prepared_judgment() {
    let mut questions = Questions::new();
    questions.insert(
        "requirement.a".into(),
        Question::Choice {
            instructions: "Validate the cited decision.".into(),
            criteria: [
                ("supported".into(), "Supported".into()),
                ("unclear".into(), "Unclear".into()),
            ]
            .into(),
        },
    );
    let state = json!({"decision":{"a":{"applicability":{"due":true}}}});
    let job = Job {
        decision: "review-identity".into(),
        worker: "worker".into(),
        state: state.clone(),
        questions: questions.clone(),
        items: vec![],
        reason: "Whole original state.".into(),
    };
    assert!(matches_decision(&job, &state, &questions).unwrap());
    assert!(
        !matches_decision(
            &job,
            &json!({"decision":{"a":{"applicability":{"due":false}}}}),
            &questions
        )
        .unwrap()
    );
    questions.insert(
        "applicability.a".into(),
        Question::Choice {
            instructions: "Validate applicability.".into(),
            criteria: [
                ("supported".into(), "Supported".into()),
                ("unclear".into(), "Unclear".into()),
            ]
            .into(),
        },
    );
    assert!(!matches_decision(&job, &state, &questions).unwrap());
}
