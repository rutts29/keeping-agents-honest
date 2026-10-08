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
