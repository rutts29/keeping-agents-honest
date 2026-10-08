use kah_cli::observer_review::{consume, state::Store};
use serde_json::{Value, json};
use std::{fs, os::unix::fs::PermissionsExt};
#[test]
fn a_failed_consume_records_a_typed_reason() {
    let worker = format!("consume-failure-{}", kah_cli::relay_store::nonce());
    let store = Store::open(&worker).unwrap();
    drop(store);
    assert!(consume::run(&worker).is_err());
    let root = Store::directory(&worker).unwrap();
    let events = fs::read_to_string(root.join("consume-outcomes.jsonl")).unwrap();
    assert!(events.contains("stage="));
    assert!(events.contains("\"success\":false"));
    fs::remove_dir_all(root.parent().unwrap()).unwrap();
}
#[test]
fn supervision_records_nonzero_exit_and_signal_without_inferring_oom() {
    for script in ["#!/bin/sh\nexit 7\n", "#!/bin/sh\nkill -KILL $$\n"] {
        let worker = format!("consume-child-{}", kah_cli::relay_store::nonce());
        let store = Store::open(&worker).unwrap();
        let program = store.directory.join("fake-kah");
        fs::write(&program, script).unwrap();
        fs::set_permissions(&program, fs::Permissions::from_mode(0o700)).unwrap();
        assert!(consume::supervisor::execute(&worker, &program).is_err());
        let events = fs::read_to_string(store.directory.join("consume-outcomes.jsonl"))
            .unwrap()
            .lines()
            .map(|l| serde_json::from_str::<Value>(l).unwrap())
            .collect::<Vec<_>>();
        let exited = events
            .iter()
            .find(|e| e["event"]["kind"] == "child_exited")
            .unwrap();
        if script.contains("exit 7") {
            assert_eq!(exited["event"]["exit_code"], json!(7));
        } else {
            assert_eq!(exited["event"]["signal"], json!(9));
        }
        let root = store.directory.parent().unwrap().to_owned();
        drop(store);
        fs::remove_dir_all(root).unwrap();
    }
}
#[test]
fn spawn_and_before_start_failures_still_record_typed_reasons() {
    let worker = format!("consume-bootstrap-{}", kah_cli::relay_store::nonce());
    let store = Store::open(&worker).unwrap();
    assert!(
        consume::supervisor::execute(
            &worker,
            std::path::Path::new("/nonexistent/kah-test-program")
        )
        .is_err()
    );
    fs::write(store.directory.join("state.json"), "not json").unwrap();
    let root = store.directory.parent().unwrap().to_owned();
    drop(store);
    assert!(consume::run(&worker).is_err());
    let events = fs::read_to_string(
        Store::directory(&worker)
            .unwrap()
            .join("consume-outcomes.jsonl"),
    )
    .unwrap();
    assert!(events.contains("REVIEW_CONSUME_SPAWN stage=supervision"));
    assert!(events.contains("before_start"));
    assert!(
        events.contains("REVIEW_STATE_JSON stage=control")
            || events.contains("REVIEW_STATE_JSON")
            || events.contains("REVIEW_STATE")
    );
    fs::remove_dir_all(root).unwrap();
}
