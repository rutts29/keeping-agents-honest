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
