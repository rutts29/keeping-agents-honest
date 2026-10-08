use kah_cli::observer_review::claims::{ReviewClass, scope};
use kah_observer::judge::{Answer, Judgment};
use std::collections::BTreeMap;

#[test]
fn only_phase_gate_probability_routes_heavy_and_smaller_claims_stay_light() {
    for (phase, implementation, expected) in [
        (0.02, 0.96, Some(ReviewClass::Commit)),
        (0.76, 0.96, Some(ReviewClass::Commit)),
        (0.80, 0.96, Some(ReviewClass::Gate)),
        (0.96, 0.02, Some(ReviewClass::Gate)),
        (0.20, 0.20, None),
        (0.21, 0.02, Some(ReviewClass::Commit)),
    ] {
        let judgment = Judgment {
            backend: "jev".into(),
            model: "jev-1.13.0".into(),
            answers: BTreeMap::from([
                ("phase_claim".into(), Answer::Noul { noul: phase }),
                (
                    "implementation_claim".into(),
                    Answer::Noul {
                        noul: implementation,
                    },
                ),
            ]),
            metrics: Default::default(),
            fallback_reason: None,
        };
        assert_eq!(scope::review_class(&judgment), expected);
    }
}

#[test]
fn native_claim_producer_uses_the_replayed_two_fact_screen() {
    use kah_cli::observer_review::claims;
    assert_eq!(claims::VERSION, scope::VERSION);
    assert_eq!(
        serde_json::to_value(claims::questions()).unwrap(),
        serde_json::to_value(scope::questions()).unwrap()
    );
    let implementation = Judgment {
        backend: "jev".into(),
        model: "jev-1.13.0".into(),
        answers: BTreeMap::from([
            ("phase_claim".into(), Answer::Noul { noul: 0.02 }),
            ("implementation_claim".into(), Answer::Noul { noul: 0.96 }),
        ]),
        metrics: Default::default(),
        fallback_reason: None,
    };
    assert!(claims::needs_review(&implementation));
    assert_eq!(
        claims::review_class(&implementation),
        Some(ReviewClass::Commit)
    );
}
