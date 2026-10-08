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
        assert_eq!(events.last().unwrap()["event"]["kind"], "child_exited");
        if script.contains("exit 7") {
            assert_eq!(events.last().unwrap()["event"]["exit_code"], json!(7));
        } else {
            assert_eq!(events.last().unwrap()["event"]["signal"], json!(9));
        }
        let root = store.directory.parent().unwrap().to_owned();
        drop(store);
        fs::remove_dir_all(root).unwrap();
    }
}
