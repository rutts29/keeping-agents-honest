use kah_cli::observer_review::{
    queue,
    state::{Purpose, Store},
    timing,
};
use serde_json::{Value, json};
#[test]
fn first_enqueue_time_survives_updates_and_snapshot_transfer_and_stages_do_not_reset() {
    let worker = format!("stage-timing-{}", std::process::id());
    let root = Store::directory(&worker).unwrap();
    queue::enqueue(
        &worker,
        Purpose::Drift,
        "first",
        json!({"actions":["first"]}),
    )
    .unwrap();
    let before: Value =
        kah_cli::observer_review::source_store::read(&root.join("queue-metadata.json")).unwrap();
    queue::enqueue(
        &worker,
        Purpose::Drift,
        "second",
        json!({"actions":["second"]}),
    )
    .unwrap();
    let updated: Value =
        kah_cli::observer_review::source_store::read(&root.join("queue-metadata.json")).unwrap();
    assert_eq!(
        updated["drift-pending.json"]["at_ms"],
        before["drift-pending.json"]["at_ms"]
    );
    let (_, paths) = queue::claim_batch(&worker).unwrap().unwrap();
    let snapshot: Value =
        kah_cli::observer_review::source_store::read(&root.join("queue-metadata.json")).unwrap();
    assert_eq!(
        snapshot[paths[0].file_name().unwrap().to_str().unwrap()]["at_ms"],
        before["drift-pending.json"]["at_ms"]
    );
    timing::mark(&worker, "review-timing", "acknowledged", 10).unwrap();
    timing::mark(&worker, "review-timing", "acknowledged", 20).unwrap();
    timing::mark(&worker, "review-timing", "completed", 30).unwrap();
    let stages: Value = kah_cli::observer_review::source_store::read(
        &root.join("stage-timings/review-timing.json"),
    )
    .unwrap();
    assert_eq!(stages["at_ms"]["acknowledged"], 10);
    assert_eq!(stages["at_ms"]["completed"], 30);
    assert!(timing::mark(&worker, "../escape", "acknowledged", 40).is_err());
    std::fs::remove_dir_all(root.parent().unwrap()).unwrap();
}
