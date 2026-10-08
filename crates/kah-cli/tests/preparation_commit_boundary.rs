use kah_cli::observer_review::{
    queue,
    state::{Purpose, Store},
};
use serde_json::json;
use std::fs;

#[test]
fn turn_preparations_wait_for_a_new_commit_without_losing_originals() {
    // Prepare at commit boundaries while retaining the first-stage screen.
    let worker = format!("commit-preparation-{}", kah_cli::relay_store::nonce());
    let store = Store::open(&worker).unwrap();
    queue::enqueue(
        &worker,
        Purpose::Preparation,
        "turn-one",
        json!({"preparation":{"purpose":"drift","questions":{},"state":{"words":"Whole user words."}}}),
    )
    .unwrap();
    let original = queue::entries(&worker).unwrap()[0].0.clone();
    let bytes = fs::read(&original).unwrap();
    assert!(queue::claim_batch(&worker).unwrap().is_none());
    assert_eq!(queue::status(&worker).unwrap()["deferred_preparations"], 1);
    assert_eq!(fs::read(&original).unwrap(), bytes);
    queue::enqueue(&worker, Purpose::Checkpoint, "commit",
        json!({"review_class":"gate","head":"aaaaaaaa","events":[{"kind":"native_head","head":"aaaaaaaa"}]})).unwrap();
    remove_checkpoint(&worker);
    assert_eq!(
        queue::claim_batch(&worker).unwrap().unwrap().0.purpose,
        Purpose::Preparation
    );
    fs::remove_file(original).unwrap();
    queue::enqueue(
        &worker,
        Purpose::Preparation,
        "turn-two",
        json!({"preparation":{"purpose":"drift","questions":{},"state":{"words":"New whole words."}}}),
    )
    .unwrap();
    queue::enqueue(
        &worker,
        Purpose::Checkpoint,
        "same-head",
        json!({"review_class":"gate","head":"aaaaaaaa"}),
    )
    .unwrap();
    remove_checkpoint(&worker);
    assert!(queue::claim_batch(&worker).unwrap().is_none());
    queue::enqueue(
        &worker,
        Purpose::Checkpoint,
        "next-commit",
        json!({"review_class":"gate","head":"bbbbbbbb"}),
    )
    .unwrap();
    remove_checkpoint(&worker);
    assert_eq!(
        queue::claim_batch(&worker).unwrap().unwrap().0.purpose,
        Purpose::Preparation
    );
    let directory = store.directory.parent().unwrap().to_owned();
    drop(store);
    fs::remove_dir_all(directory).unwrap();
}

fn remove_checkpoint(worker: &str) {
    for (path, wake) in queue::entries(worker).unwrap() {
        if wake.purpose == Purpose::Checkpoint {
            fs::remove_file(path).unwrap();
        }
    }
}

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
