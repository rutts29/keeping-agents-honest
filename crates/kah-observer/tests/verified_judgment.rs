use kah_observer::{
    judge::{Answer, Judgment, Metrics, decision::AssessmentState},
    verified_judgment,
};
use serde_json::json;
use std::collections::BTreeMap;
#[test]
fn independently_confirmed_polishing_and_unsupported_claims_can_nudge() {
    for (signal, score) in [
        ("polishing_instead_of_progress", 0.95),
        ("claim_supported", 0.05),
    ] {
        let mut j = Judgment {
            backend: "jev".into(),
            model: "jev-latest".into(),
            answers: BTreeMap::from([(signal.into(), Answer::Noul { noul: score })]),
            metrics: Metrics::default(),
            fallback_reason: None,
        };
        assert_eq!(
            verified_judgment::assess(&j, &[json!({"id":"verified-1"})], 0).state,
            AssessmentState::MaterialDrift
        );
        j.answers.insert(signal.into(), Answer::Noul { noul: 0.5 });
        assert_ne!(
            verified_judgment::assess(&j, &[json!({"id":"verified-1"})], 0).state,
            AssessmentState::MaterialDrift
        );
        j.backend = "model_report".into();
        j.answers
            .insert(signal.into(), Answer::Noul { noul: score });
        assert_ne!(
            verified_judgment::assess(&j, &[json!({"id":"verified-1"})], 0).state,
            AssessmentState::MaterialDrift
        );
    }
}
