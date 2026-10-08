//! Lossless storage only: whole native records retain their order and identity.
use kah_cli::observer_review::source_store::{self, Select};
use serde_json::{Value, json};
use std::fs;
#[test]
fn appended_native_records_reuse_prior_skeletons_and_preserve_every_field() {
    let root = std::env::temp_dir().join(format!("kah-record-cas-{}", std::process::id()));
    fs::create_dir(&root).unwrap();
    source_store::initialize(&root, &root.join("workspace")).unwrap();
    let mut records=(0..400).map(|id|json!({"id":format!("native-{id}"),"kind":"tool_result","call":format!("call-{id}"),"output":"Whole native output 🦀.\n".repeat(20),"exitCode":0})).collect::<Vec<_>>();
    let first = json!({"state":{"event":{"actions":records}}});
    source_store::save(&root, "first.json", &first).unwrap();
    let old = stored_records(&root.join("first.json"));
    assert!(
        old.iter()
            .all(|record| record["$kah_source"].is_string() && record["encoding"] == "json")
    );
    records.push(
        json!({"id":"native-400","kind":"tool_result","output":"New whole output","exitCode":1}),
    );
    let second = json!({"state":{"event":{"actions":records}}});
    source_store::save(&root, "second.json", &second).unwrap();
    let new = stored_records(&root.join("second.json"));
    assert_eq!(&new[..old.len()], old);
    assert_eq!(
        source_store::read::<Value>(&root.join("first.json")).unwrap(),
        first
    );
    assert_eq!(
        source_store::read::<Value>(&root.join("second.json")).unwrap(),
        second
    );
    let digest = new[0]["$kah_source"].as_str().unwrap();
    fs::write(root.join("sources").join(digest), b"corrupt record").unwrap();
    assert!(source_store::read::<Value>(&root.join("second.json")).is_err());
    fs::remove_dir_all(root).unwrap();
}
fn stored_records(path: &std::path::Path) -> Vec<Value> {
    let selected = source_store::read_selected(
        path,
        &Select::fields(&[(
            "state",
            Select::fields(&[(
                "event",
                Select::fields(&[("actions", Select::Each(Box::new(Select::Raw)))]),
            )]),
        )]),
    )
    .unwrap();
    selected["state"]["event"]["actions"]
        .as_array()
        .unwrap()
        .clone()
}
