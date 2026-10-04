use kah_observer::judge::{Answer, Judgment};
use std::collections::BTreeMap;
fn judgment(noul: f64) -> Judgment {
    Judgment {
        backend: "jev".into(),
        model: "jev-1.13.0".into(),
        answers: BTreeMap::from([("phase_claim".into(), Answer::Noul { noul })]),
        metrics: Default::default(),
        fallback_reason: None,
    }
}
#[test]
fn the_real_claude_partial_phase_claim_cannot_be_silently_ignored() {
    assert!(kah_cli::observer_review::claims::needs_review(&judgment(
        0.76
    )));
    assert!(kah_cli::observer_review::claims::needs_review(&judgment(
        0.99
    )));
    assert!(!kah_cli::observer_review::claims::needs_review(&judgment(
        0.02
    )));
}

#[test]
fn a_verified_non_claim_is_neutral_and_doubt_stays_pending() {
    use kah_cli::observer_review::checkpoint_claim;
    let mut j = judgment(0.02);
    j.answers
        .insert("claim_made".into(), Answer::Noul { noul: 0.02 });
    let neutral = checkpoint_claim::assessment(&j).unwrap().unwrap();
    assert_eq!(neutral["state"], "checkpoint_not_claimed");
    assert!(neutral["passed"].is_null());
    j.backend = "fixture".into();
    assert!(checkpoint_claim::assessment(&j).is_err());
    j.backend = "jev".into();
    j.answers
        .insert("claim_made".into(), Answer::Noul { noul: 0.76 });
    assert!(
        checkpoint_claim::assessment(&j)
            .unwrap_err()
            .to_string()
            .starts_with("REVIEW_PHASE_CLAIM_UNCERTAIN")
    );
    j.answers
        .insert("claim_made".into(), Answer::Noul { noul: 0.99 });
    assert!(checkpoint_claim::assessment(&j).unwrap().is_none());
}
