//! Legacy commit wakes migrate to light reviews; explicit gates do not.
use kah_cli::observer_review::{
    queue::{Wake, migration},
    state::Purpose,
};
use serde_json::json;

fn wake(events: serde_json::Value) -> Wake {
    Wake {
        purpose: Purpose::Checkpoint,
        turn: "historical".into(),
        trigger: json!({"events":events}),
    }
}
#[test]
fn legacy_commit_and_uncertain_claims_are_light_but_confirmed_and_mixed_gates_are_not() {
    for events in [
        json!([{"kind":"native_head","head":"a".repeat(40)}]),
        json!([{"kind":"backlog_latest_commit","head":"a".repeat(40)}]),
        json!([{"kind":"native_phase_claim","judgment":{"backend":"jev","answers":{"phase_claim":{"type":"noul","noul":0.34}}}}]),
    ] {
        assert!(migration::eligible(&wake(events)));
    }
    for event in [
        json!({"kind":"native_git_tag"}),
        json!({"kind":"user_gate"}),
        json!({"kind":"run_completion"}),
        json!({"kind":"native_phase_claim","judgment":{"backend":"jev","answers":{"phase_claim":{"type":"noul","noul":0.8}}}}),
        json!({"kind":"native_phase_claim","judgment":{"backend":"jev","answers":{"phase_claim":{"type":"noul","noul":0.2}}}}),
        json!({"kind":"unknown"}),
    ] {
        assert!(!migration::eligible(&wake(json!([event.clone()]))));
        assert!(!migration::eligible(&wake(
            json!([{"kind":"native_head","head":"a".repeat(40)},event])
        )));
    }
}
#[test]
fn conversion_preserves_claim_and_authority_without_inventing_a_historical_head() {
    let mut original = wake(
        json!([{"kind":"native_phase_claim","judgment":{"backend":"jev","answers":{"phase_claim":{"type":"noul","noul":0.73}}},"source":{"whole":"Native original."}}]),
    );
    original.trigger["user_authority_change"] =
        json!({"receipt":"user-real","text":"Whole user words."});
    let converted = migration::convert(&original, "authorized").unwrap();
    assert_eq!(converted.trigger["review_class"], "commit");
    assert!(converted.trigger["head"].is_null());
    assert_eq!(converted.trigger["events"], original.trigger["events"]);
    assert_eq!(
        converted.trigger["user_authority_change"],
        original.trigger["user_authority_change"]
    );
    assert_eq!(
        converted.trigger["legacy_light_migration"]["original_trigger"],
        original.trigger
    );
}
