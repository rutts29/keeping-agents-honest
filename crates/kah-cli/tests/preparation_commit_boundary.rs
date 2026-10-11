use kah_cli::observer_review::{
    queue,
    state::{Purpose, Store},
};
use serde_json::json;
use std::fs;

#[test]
fn drift_tripwire_and_native_snapshot_preparation_keep_their_paths() {
    let worker = format!("commit-tripwire-{}", kah_cli::relay_store::nonce());
    let store = Store::open(&worker).unwrap();
    let triggers =
        kah_cli::observer_review::dispatch::triggers(Some(json!({"tripwire":"uncertain"})), None);
    assert_eq!(triggers[0].0, Purpose::Drift);
    queue::enqueue(&worker, Purpose::Drift, "tool-four", triggers[0].1.clone()).unwrap();
    assert_eq!(
        queue::claim_batch(&worker).unwrap().unwrap().0.purpose,
        Purpose::Drift
    );
    for (path, _) in queue::entries(&worker).unwrap() {
        fs::remove_file(path).unwrap();
    }
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
