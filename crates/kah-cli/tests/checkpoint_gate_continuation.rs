//! Recorded queue control only; no native sessions, decision rendering or Jev.
use kah_cli::observer_review::{
    queue::{self, Wake},
    source_store,
    state::{Purpose, Store},
};
use serde_json::{Value, json};
use std::fs;

fn retained() -> Value {
    serde_json::from_str(include_str!(
        "../../../docs/gates/evidence/phase3-claude/b2/same-sha-terminal/queue-replay.json"
    ))
    .unwrap()
}

#[test]
fn retained_typed_gate_continuation_keeps_parent_order_ahead_of_tag_and_completion() {
    std::env::var_os("XDG_STATE_HOME").expect("run through demos/check-tests.sh");
    let (store, fixture, wake, receipt) = replay_queue();
    let worker = wake.trigger["captured_snapshot"]["worker"]
        .as_str()
        .unwrap();
    let root = store.directory.clone();
    assert!(queue::enqueue_once_at(&root, &receipt, &wake).unwrap());
    let jobs = queue::entries(worker).unwrap();
    assert_eq!(jobs.len(), 3);
    assert_eq!(
        jobs[0].1.trigger["captured_snapshot"]["id"],
        fixture["typed_header"]["id"]
    );
    assert_eq!(jobs[1].1.trigger, fixture["tag"]["trigger"]);
    assert_eq!(jobs[2].1.trigger, fixture["completion"]["trigger"]);
    assert!(!queue::enqueue_once_at(&root, &receipt, &wake).unwrap());
    assert_eq!(queue::entries(worker).unwrap().len(), 3);
    drop(store);
    fs::remove_dir_all(root.parent().unwrap()).unwrap();
}

fn replay_queue() -> (Store, Value, Wake, String) {
    let fixture = retained();
    let worker = format!("gate-continuation-{}", kah_cli::relay_store::nonce());
    let store = Store::open(&worker).unwrap();
    let root = store.directory.clone();
    kah_core::run_store::ensure_private_directory(&root.join("queued")).unwrap();
    source_store::save(&root, "queue-order.json", &fixture["order"]).unwrap();
    source_store::save(&root, "queue-metadata.json", &fixture["metadata"]).unwrap();
    for (name, value) in [("tag_name", "tag"), ("completion_name", "completion")] {
        source_store::save(
            &root.join("queued"),
            fixture[name].as_str().unwrap(),
            &fixture[value],
        )
        .unwrap();
    }
    let mut captured = fixture["typed_header"].clone();
    captured["worker"] = worker.clone().into();
    // New host control metadata binds a continuation to its admitted parent.
    // The original retained parent was ordinal3; tag/end were5/7, child was8.
    captured["trigger"]["queued_receipt"] = fixture["parent_name"].clone();
    let wake = Wake {
        purpose: Purpose::Checkpoint,
        turn: captured["turn"].as_str().unwrap().into(),
        trigger: json!({"captured_snapshot":captured}),
    };
    let receipt = format!(
        "explicit-gate:{}",
        fixture["parent_request"].as_str().unwrap()
    );
    (store, fixture, wake, receipt)
}
