use super::*;
use crate::judge::{Answer, Metrics};
fn case() -> Case {
    let row:Row=serde_json::from_value(json!({"id":"x-3","session":"s","ts":"now","source_kind":"user","user_text":"Add a check","prev_assistant":"","prior_items":[],"gold":{"is_steering":true,"steer_kind":"new_constraint","supersedes":"none","tags":["fixture"],"note":"synthetic"}})).unwrap();
    dataset::prepare(vec![row], false).unwrap().remove(0)
}
#[test]
fn metrics_count_abstentions_and_threshold_sweeps_without_changing_policy() {
    let before = judge::questions::STEERING_PROBABILITY;
    let mut score = scoring::Scores::default();
    let mut record = Record {
        id: "x-3".into(),
        request_digest: "x".into(),
        source: "user".into(),
        judgment: None,
        error: Some("fixture unavailable".into()),
    };
    assert!(!score.add(&case(), &record));
    let report = score.report();
    assert_eq!(report["perQuestion"]["is_steering"]["abstained"], 1);
    assert_eq!(report["perQuestion"]["is_steering"]["accuracy"], 0.0);
    assert!(report["perQuestion"]["is_steering"]["accuracyWhenAnswered"].is_null());
    record.error = None;
    record.judgment = Some(Judgment {
        backend: "jev".into(),
        model: "fake".into(),
        answers: BTreeMap::from([
            ("is_steering".into(), Answer::Noul { noul: 0.7 }),
            (
                "steer_kind".into(),
                Answer::Choice {
                    choice: "new_constraint".into(),
                    probabilities: BTreeMap::from([("new_constraint".into(), 0.99)]),
                    confidence: 0.99,
                },
            ),
        ]),
        metrics: Metrics {
            calls: 1,
            input_tokens: 50,
            elapsed_ms: 25,
            ..Metrics::default()
        },
        fallback_reason: None,
    });
    assert!(score.add(&case(), &record));
    let report = score.report();
    assert_eq!(
        report["thresholdSweeps"]["is_steering_probability_0.50"]["correct"],
        1
    );
    assert_eq!(
        report["thresholdSweeps"]["is_steering_probability_0.80"]["correct"],
        0
    );
    assert_eq!(report["metrics"]["callsRepresented"], 1);
    assert_eq!(judge::questions::STEERING_PROBABILITY, before);
}
