use kah_observer::{
    checkpoint,
    judge::{Answer, Judgment, Metrics},
};
use std::collections::BTreeMap;
fn satisfied() -> Judgment {
    Judgment {
        backend: "jev".into(),
        model: "jev-latest".into(),
        answers: ["intent", "end_to_end", "work_accounted", "drift"]
            .into_iter()
            .map(|id| {
                (
                    id.into(),
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
fn individually_passing_phases_cannot_override_failed_cumulative_integration() {
    let mut judgment = satisfied();
    assert!(checkpoint::decide(&judgment).passed);
    judgment.answers.insert(
        "end_to_end".into(),
        Answer::Choice {
            choice: "no".into(),
            confidence: 0.95,
            probabilities: BTreeMap::from([
                ("no".into(), 0.98),
                ("yes".into(), 0.01),
                ("unclear".into(), 0.01),
            ]),
        },
    );
    assert_eq!(checkpoint::decide(&judgment).unproven, ["integration"]);
}
#[test]
fn every_dimension_requires_the_existing_completion_thresholds_and_jev() {
    for id in ["intent", "end_to_end", "work_accounted", "drift"] {
        let mut judgment = satisfied();
        if let Answer::Choice { confidence, .. } = judgment.answers.get_mut(id).unwrap() {
            *confidence = 0.84;
        }
        assert!(!checkpoint::decide(&judgment).passed);
        judgment.answers.remove(id);
        assert!(!checkpoint::decide(&judgment).passed);
    }
    let mut judgment = satisfied();
    judgment.backend = "untrusted_model_claim".into();
    assert!(!checkpoint::decide(&judgment).passed);
    assert_eq!(checkpoint::questions().len(), 6);
}
