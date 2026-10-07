use kah_cli::observer_review::{redelivery, state::Store};
use serde_json::json;
#[test]
fn unchanged_missing_historical_anchor_after_correction_parks_instead_of_looping() {
    let worker = format!("correction-loop-{}", std::process::id());
    let store = Store::open(&worker).unwrap();
    let gap = json!({"request":"review-host-anchor","origin":"report_validation","error":"REVIEW_SOURCE_CHANGED stage=evidence","request_digest":"original-request","report_digest":"unchanged-report"});
    assert!(!redelivery::unchanged_correction(&store, "review-host-anchor", &gap).unwrap());
    let attempt =
        redelivery::begin(&store, "review-host-anchor", gap["error"].as_str().unwrap()).unwrap();
    redelivery::bind_correction(&store, "review-host-anchor", attempt, &gap).unwrap();
    redelivery::outcome(&store, "review-host-anchor", attempt, None).unwrap();
    assert!(redelivery::unchanged_correction(&store, "review-host-anchor", &gap).unwrap());
    redelivery::park_correction(&store, "review-host-anchor", &gap).unwrap();
    let notice = redelivery::take(&store).unwrap().unwrap();
    assert!(
        notice["systemMessage"]
            .as_str()
            .unwrap()
            .contains("review-host-anchor")
    );
    redelivery::park_correction(&store, "review-host-anchor", &gap).unwrap();
    assert!(
        redelivery::take(&store).unwrap().is_none(),
        "one notice per unchanged block"
    );
    let mut changed = gap.clone();
    changed["report_digest"] = "new-report".into();
    assert!(!redelivery::unchanged_correction(&store, "review-host-anchor", &changed).unwrap());
    changed = gap.clone();
    changed["request_digest"] = "repaired-anchor-manifest".into();
    assert!(!redelivery::unchanged_correction(&store, "review-host-anchor", &changed).unwrap());
    assert!(!redelivery::unchanged_correction(&store, "review-other", &gap).unwrap());
    std::fs::remove_dir_all(store.directory.parent().unwrap()).unwrap();
}
#[test]
fn parked_validation_releases_its_slot_and_retains_the_pending_request() {
    use kah_cli::observer_review::state::State;
    let worker = format!("parked-slot-{}", kah_cli::relay_store::nonce());
    let store = Store::open(&worker).unwrap();
    let state: State = serde_json::from_value(json!({"worker":{"provider":"codex","session":worker,"project":"/tmp/fixture","pane":null,"socket":null},"observer":{"session":null,"pane":null,"name":"fixture","workspace":"/tmp/fixture","model":"gpt-6.1-sol","effort":"low","transcript":null},"pending":{"id":"review-stuck","purpose":"checkpoint","turn":"t","head":"a","created_ms":1,"native_tokens_before":null},"latest":null,"last_verified_turn":null,"checkpoint":null,"deferrals":[],"wakes":0,"native_tokens":0,"stopped":false})).unwrap();
    store.save(&state).unwrap();
    let gap = json!({"error":"REVIEW_REQUIREMENT_EMPTY stage=decision_units","report_digest":"same","request_digest":"same"});
    kah_cli::unattended_report::save(&store.directory, "gap.json", &gap).unwrap();
    redelivery::park_correction(&store, "review-stuck", &gap).unwrap();
    assert!(store.load().unwrap().unwrap().pending.is_none());
    let parked: serde_json::Value = kah_cli::observer_review::source_store::read(
        &store.directory.join("parked-validation/review-stuck.json"),
    )
    .unwrap();
    assert_eq!(parked["pending"]["id"], "review-stuck");
    assert_eq!(parked["gap"], gap);
    assert!(
        !store.directory.join("gap.json").exists(),
        "old gap must not correct the next review"
    );
    assert_ne!(
        store.load().unwrap().unwrap().checkpoint.unwrap()["passed"],
        true
    );
    let root = store.directory.parent().unwrap().to_owned();
    drop(store);
    std::fs::remove_dir_all(root).unwrap();
}
