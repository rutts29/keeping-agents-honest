use kah_cli::observer_review::{
    queue::{self, preparation_coverage},
    state::{Purpose, Store},
};
use serde_json::json;
use std::fs;
#[test]
fn coverage_retires_exact_packet_with_a_link_and_keeps_partial_or_newer_packets() {
    let worker = format!("coverage-store-{}", kah_cli::relay_store::nonce());
    let store = Store::open(&worker).unwrap();
    let intent =
        json!({"revision":3,"goal":"Whole goal","items":[["user-one","Whole user words"]]});
    let drift =
        preparation_coverage::stamp(&worker, json!({"actions":[{"id":"a"},{"id":"b"}]}), &intent);
    queue::enqueue(&worker, Purpose::Drift, "full", drift).unwrap();
    for (turn, ids, revision) in [
        ("covered", vec!["a", "b"], 3),
        ("partial", vec!["a", "missing"], 3),
        ("newer-authority", vec!["a"], 4),
    ] {
        queue::enqueue(&worker,Purpose::Preparation,turn,json!({"preparation":{"purpose":"drift","state":{
            "intent":{"revision":revision,"goal":"Whole goal","items":[["user-one","Whole user words"]]},
            "event":{"actions":ids.iter().map(|id|json!({"id":id})).collect::<Vec<_>>()}}}})).unwrap();
    }
    let jobs = queue::entries(&worker).unwrap();
    let original = jobs
        .iter()
        .find(|(_, w)| w.turn == "covered")
        .unwrap()
        .0
        .clone();
    let bytes = fs::read(&original).unwrap();
    let result = preparation_coverage::run(&worker).unwrap();
    assert_eq!(result["retired"].as_array().unwrap().len(), 1);
    assert_eq!(result["kept"].as_array().unwrap().len(), 2);
    assert_eq!(result["before"], 4);
    assert_eq!(result["after"], 3);
    assert!(
        result["retired"][0]["covered_by"]["reference"]
            .as_str()
            .unwrap()
            .starts_with("queued:")
    );
    assert_eq!(
        fs::read(
            store
                .directory
                .join("queue-retired")
                .join(original.file_name().unwrap())
        )
        .unwrap(),
        bytes
    );
    assert!(!original.exists());
    assert_eq!(
        preparation_coverage::run(&worker).unwrap()["retired"],
        json!([])
    );
    let root = store.directory.parent().unwrap().to_owned();
    drop(store);
    fs::remove_dir_all(root).unwrap();
}
