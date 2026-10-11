use kah_cli::observer_review::{
    queue,
    state::{Purpose, Store},
};
use serde_json::json;
use std::fs;

#[test]
fn native_snapshot_preparation_keeps_its_exact_gate_binding() {
    let worker = format!("commit-tripwire-{}", kah_cli::relay_store::nonce());
    let store = Store::open(&worker).unwrap();
    queue::enqueue(
        &worker,
        Purpose::Preparation,
        "gate",
        json!({"captured_snapshot":{"id":"exact-gate"}}),
    )
    .unwrap();
    assert_eq!(
        queue::claim_batch(&worker).unwrap().unwrap().0.purpose,
        Purpose::Preparation
    );
    let root = store.directory.parent().unwrap().to_owned();
    drop(store);
    fs::remove_dir_all(root).unwrap();
}
