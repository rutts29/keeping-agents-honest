use kah_cli::observer_review::claims::{self, ReviewClass};
use kah_observer::judge::{Answer, Judgment, Metrics};
use std::collections::BTreeMap;
fn judgment(probability: f64) -> Judgment {
    Judgment {
        backend: "jev".into(),
        model: "jev-1.13.0".into(),
        answers: BTreeMap::from([("phase_claim".into(), Answer::Noul { noul: probability })]),
        metrics: Metrics::default(),
        fallback_reason: None,
    }
}
#[test]
fn unchanged_ignore_threshold_routes_uncertain_claims_light_and_confirmed_claims_heavy() {
    assert_eq!(claims::review_class(&judgment(0.2)), None);
    assert_eq!(
        claims::review_class(&judgment(0.200001)),
        Some(ReviewClass::Commit)
    );
    assert_eq!(
        claims::review_class(&judgment(0.799999)),
        Some(ReviewClass::Commit)
    );
    assert_eq!(
        claims::review_class(&judgment(0.8)),
        Some(ReviewClass::Gate)
    );
    assert_eq!(
        claims::review_class(&judgment(0.99)),
        Some(ReviewClass::Gate)
    );
    let mut unavailable = judgment(0.99);
    unavailable.backend = "not_requested".into();
    assert_eq!(claims::review_class(&unavailable), None);
}
