use kah_cli::observer_review::{
    queue::{Wake, claim_migration},
    state::Purpose,
};
use kah_observer::judge::{Answer, Judgment};
use serde_json::json;
use std::collections::BTreeMap;
fn wake() -> Wake {
    Wake {
        purpose: Purpose::Checkpoint,
        turn: "old-turn".into(),
        trigger: json!({"review_class":"gate","events":[{"kind":"native_phase_claim","head":"a".repeat(40),"question_set":2,"source":{"native_assistant_text":"Fix installed; Phase5O remains unproven."},"judgment":{"old":"whole record"}}],"user_authority_change":{"receipt":"real-authority","text":"Whole user words."}}),
    }
}
fn judged(phase: f64, implementation: f64) -> Judgment {
    Judgment {
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
    }
}
#[test]
fn a_legacy_implementation_becomes_light_with_original_source_and_authority() {
    let original = wake();
    let converted = claim_migration::reclassify(&original, &[judged(0.34, 0.96)])
        .unwrap()
        .unwrap();
    assert_eq!(converted.trigger["review_class"], "commit");
    assert_eq!(converted.trigger["head"], "a".repeat(40));
    assert_eq!(converted.trigger["events"][0]["uncertain"], true);
    assert_eq!(
        converted.trigger["events"][0]["source"],
        original.trigger["events"][0]["source"]
    );
    assert_eq!(
        converted.trigger["user_authority_change"],
        original.trigger["user_authority_change"]
    );
    assert_eq!(
        converted.trigger["events"][0]["previous_classification"]["judgment"],
        original.trigger["events"][0]["judgment"]
    );
    assert!(!claim_migration::eligible(&converted));
}
#[test]
fn phase_stays_heavy_ack_is_explicitly_unclaimed_and_explicit_gates_are_ineligible() {
    let mut original = wake();
    original.trigger["events"][0]["uncertain"] = json!(true);
    let confirmed = claim_migration::reclassify(&original, &[judged(0.98, 0.92)])
        .unwrap()
        .unwrap();
    assert_eq!(confirmed.trigger["events"][0]["uncertain"], false);
    assert_eq!(
        claim_migration::reclassify(&original, &[judged(0.98, 0.92)])
            .unwrap()
            .unwrap()
            .trigger["review_class"],
        "gate"
    );
    assert!(
        claim_migration::reclassify(&original, &[judged(0.09, 0.04)])
            .unwrap()
            .is_none()
    );
    for kind in ["native_git_tag", "user_gate", "run_completion"] {
        let mut mixed = original.clone();
        mixed.trigger["events"]
            .as_array_mut()
            .unwrap()
            .push(json!({"kind":kind}));
        assert!(!claim_migration::eligible(&mixed));
    }
    let mut invalid = judged(0.02, 0.98);
    invalid.backend = "fixture".into();
    assert!(claim_migration::reclassify(&original, &[invalid]).is_err());
}
