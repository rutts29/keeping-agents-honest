use kah_observer::{
    judge::{Answer, Judgment},
    verification_contract::reasoned,
};
use std::collections::BTreeMap;

fn judgment(choice: &str, probability: f64, confidence: f64) -> Judgment {
    let other = if choice == "supported" {
        "contradicted"
    } else {
        "supported"
    };
    let answer = Answer::Choice {
        choice: choice.into(),
        probabilities: BTreeMap::from([
            (choice.into(), probability),
            (other.into(), 1.0 - probability),
        ]),
        confidence,
    };
    Judgment {
        backend: "jev".into(),
        model: "jev-1.13.0".into(),
        answers: BTreeMap::from([("coverage_decision.item".into(), answer)]),
        metrics: Default::default(),
        fallback_reason: None,
    }
}

#[test]
fn supported_coverage_confirms_and_unsupported_or_doubtful_coverage_never_does() {
    // A check that exercises the requirement.
    assert!(reasoned::covered(
        &judgment("supported", 0.98, 0.97),
        "item"
    ));
    // A command that only prints success.
    assert!(!reasoned::covered(
        &judgment("contradicted", 0.74, 0.61),
        "item"
    ));
    // Supported, but below the confidence and probability thresholds.
    assert!(!reasoned::covered(&judgment("supported", 0.7, 0.6), "item"));
    assert!(!reasoned::covered(
        &judgment("supported", 0.98, 0.97),
        "other"
    ));
}
