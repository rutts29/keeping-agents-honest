use kah_cli::observer_review::{
    queue,
    state::{Purpose, Store},
    timing,
};
use serde_json::{Value, json};
#[test]
fn every_admission_retains_its_own_receipt_when_latest_lane_is_replaced() {
    use kah_cli::observer_review::state::Request;
    let worker = format!("admission-receipt-{}", std::process::id());
    // Real admission has already opened the session store and its source root.
    drop(Store::open(&worker).unwrap());
    for id in ["review-first", "review-second"] {
        let request = Request {
            id: id.into(),
            worker: worker.clone(),
            purpose: Purpose::Checkpoint,
            turn: "t".into(),
            head: None,
            intent: Value::Null,
            trigger: json!({"review_class":"commit"}),
            documents: vec![],
            created_ms: 1,
        };
        queue::metadata::admitted(&worker, &request).unwrap();
    }
    let root = Store::directory(&worker).unwrap();
    for id in ["review-first", "review-second"] {
        let receipt: Value = kah_cli::observer_review::source_store::read(
            &root.join(format!("stage-timings/{id}.json")),
        )
        .unwrap();
        assert!(receipt["at_ms"]["admitted"].is_u64());
        assert!(root.join(format!("stage-timings/{id}.jsonl")).is_file());
    }
    std::fs::remove_dir_all(root.parent().unwrap()).unwrap();
}
#[test]
fn delayed_native_stop_records_its_own_request_and_original_host_receipt_time() {
    use kah_cli::observer_review::{hook_queue, request_context::Context};
    let worker = format!("native-end-timing-{}", std::process::id());
    let mut event = hook_queue::Event::capture(
        json!({"hook_event_name":"Stop","session_id":"observer"}),
        None,
    );
    event.received_ms = Some(123);
    event.context = Some(Context {
        request: "review-old".into(),
        preparation: false,
    });
    timing::native_end(&worker, &event).unwrap();
    let root = Store::directory(&worker).unwrap();
    let record: Value =
        kah_cli::observer_review::source_store::read(&root.join("stage-timings/review-old.json"))
            .unwrap();
    assert_eq!(record["at_ms"]["native_done"], 123);
    assert!(!root.join("stage-timings/review-new.json").exists());
    event.context = Some(Context {
        request: "review-legacy".into(),
        preparation: false,
    });
    event.received_ms = None;
    timing::native_end(&worker, &event).unwrap();
    assert!(!root.join("stage-timings/review-legacy.json").exists());
    std::fs::remove_dir_all(root.parent().unwrap()).unwrap();
}
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
    let history = std::fs::read_to_string(root.join("stage-timings/review-timing.jsonl")).unwrap();
    let rows = history
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).unwrap())
        .collect::<Vec<_>>();
    assert_eq!(
        rows.iter()
            .map(|row| row["at_ms"].as_u64().unwrap())
            .collect::<Vec<_>>(),
        [10, 20, 30]
    );
    assert!(timing::mark(&worker, "../escape", "acknowledged", 40).is_err());
    std::fs::remove_dir_all(root.parent().unwrap()).unwrap();
}
#[test]
fn first_append_and_cached_metadata_recover_retained_original_age() {
    let worker = format!("repaired-age-{}", std::process::id());
    let root = Store::directory(&worker).unwrap();
    for name in ["queue-retired", "queue-repairs"] {
        kah_core::run_store::ensure_private_directory(&root.join(name)).unwrap();
    }
    std::fs::write(
        root.join("queue-retired/original.json"),
        "retained original",
    )
    .unwrap();
    let original_at = std::fs::metadata(root.join("queue-retired/original.json"))
        .unwrap()
        .modified()
        .unwrap()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64;
    queue::enqueue(
        &worker,
        Purpose::Drift,
        "first",
        json!({"actions":["first"]}),
    )
    .unwrap();
    kah_cli::observer_review::source_store::save(
        &root,
        "queue-metadata.json",
        &json!({"drift-pending.json":{"at_ms":original_at+10000,"lane":"intent"}}),
    )
    .unwrap();
    kah_cli::observer_review::source_store::save(
        &root.join("queue-repairs"),
        "repair.json",
        &json!({"rows":[{"original":"original.json","replacement":"drift-pending.json"}]}),
    )
    .unwrap();
    queue::entries(&worker).unwrap();
    let recovered: Value =
        kah_cli::observer_review::source_store::read(&root.join("queue-metadata.json")).unwrap();
    assert_eq!(recovered["drift-pending.json"]["at_ms"], original_at);
    std::fs::remove_file(root.join("queue-metadata.json")).unwrap();
    queue::enqueue(
        &worker,
        Purpose::Drift,
        "later",
        json!({"actions":["later"]}),
    )
    .unwrap();
    let append: Value =
        kah_cli::observer_review::source_store::read(&root.join("queue-metadata.json")).unwrap();
    assert_eq!(append["drift-pending.json"]["at_ms"], original_at);
    std::fs::remove_dir_all(root.parent().unwrap()).unwrap();
}
