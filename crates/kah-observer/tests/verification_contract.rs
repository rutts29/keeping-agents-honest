use kah_observer::{
    judge::{Answer, Judgment, Metrics},
    verification_contract::{Item, Method},
};
use std::collections::BTreeMap;
fn item() -> Item {
    Item {
        requirement_id: "normalize_contract".into(),
        claim: "The named normalize export computes the requested rounding map.".into(),
        evidence_citations: vec![0],
        coverage_reasoning: "Exercise the export and assert its results.".into(),
        method: Method::TargetedCheck {
            command: "node -e \"console.log('PASS')\"".into(),
        },
    }
}
fn judgment(subject: &str, assertion: &str) -> Judgment {
    Judgment {
        backend: "jev".into(),
        model: "jev-latest".into(),
        answers: [
            ("coverage_subject", subject),
            ("coverage_assertion", assertion),
        ]
        .into_iter()
        .map(|(k, v)| {
            (
                k.into(),
                Answer::Choice {
                    choice: v.into(),
                    confidence: 0.99,
                    probabilities: BTreeMap::from([(v.into(), 0.99)]),
                },
            )
        })
        .collect(),
        metrics: Metrics::default(),
        fallback_reason: None,
    }
}
#[test]
fn passing_without_touching_the_export_is_not_verification() {
    let i = item();
    assert!(!i.covered(&judgment("unsupported", "unsupported")));
    assert!(!i.covered(&judgment("supported", "unclear")));
    assert!(i.covered(&judgment("supported", "supported")));
}
#[test]
fn missing_executable_coverage_never_becomes_a_user_risk() {
    let mut i = item();
    i.method = Method::NeedsCheck {
        why: "No check exercises this export yet.".into(),
    };
    assert!(!i.covered(&judgment("supported", "supported")));
    assert_eq!(i.command(&[]).unwrap(), None);
}
