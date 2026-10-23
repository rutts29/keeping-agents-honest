//! Queue control only; no native sessions, decision rendering or Jev.
#[allow(dead_code)]
mod checkpoint_replay;
use kah_cli::observer_review::{
    queue::{self, Wake},
    source_store,
    state::{Purpose, Store},
};
use serde_json::{Value, json};
use std::fs;

/// A queue holding an admitted gate (the parent), then a tag and a run end.
/// Commit-lane entries interleave; only the gate lane is ordered here.
fn queued() -> Value {
    let name = |ordinal: u64| format!("{ordinal:032x}-4242.json");
    let lanes = [
        (1, "intent"),
        (2, "commit"),
        (3, "gate"),
        (4, "commit"),
        (5, "gate"),
        (6, "commit"),
        (7, "gate"),
    ];
    let order = lanes
        .iter()
        .map(|(n, _)| (name(*n), json!(n)))
        .collect::<serde_json::Map<_, _>>();
    let metadata = lanes
        .iter()
        .map(|(n, lane)| {
            (
                name(*n),
                json!({"at_ms":1_700_000_000_000u64 + n * 1000,"lane":lane}),
            )
        })
        .collect::<serde_json::Map<_, _>>();
    let worker = "gate-continuation-worker";
    let mut typed = checkpoint_replay::gate_request(worker);
    typed["id"] = "review-1700000500000-501".into();
    typed["trigger"]["decision_protocol_version"] = 12.into();
    typed["trigger"]["option1_cumulative_input"] = json!({"head":checkpoint_replay::HEAD,
        "intent_revision":2,"request":checkpoint_replay::GATE_REQUEST});
    json!({"worker":worker,"head":checkpoint_replay::HEAD,"parent_request":checkpoint_replay::GATE_REQUEST,
        "parent_name":name(3),"tag_name":name(5),"completion_name":name(7),"order":order,"metadata":metadata,
        "typed_header":typed,
        "tag":{"purpose":"checkpoint","turn":"tag-turn","trigger":checkpoint_replay::tag_trigger()},
        "completion":{"purpose":"checkpoint","turn":"end-turn","trigger":checkpoint_replay::completion_trigger()}})
}

#[test]
fn typed_gate_continuation_keeps_parent_order_ahead_of_tag_and_completion() {
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
    let fixture = queued();
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
    // Host control metadata binds a continuation to its admitted parent:
    // the parent is ordinal 3, the tag and run end 5 and 7.
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
