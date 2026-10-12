//! Native partial appends and metadata reuse, no provider or pane calls.
use kah_cli::observer_review::{
    provider_block, source_store,
    state::{State, Store},
};
use serde_json::{Value, json};
use std::{fs, io::Write};
#[test]
fn only_new_complete_native_records_are_examined() {
    std::env::var_os("XDG_STATE_HOME").expect("private check-tests state required");
    let worker = format!("provider-cursor-{}", kah_cli::relay_store::nonce());
    let store = Store::open(&worker).unwrap();
    let root = store.directory.clone();
    let native = root.join("native.jsonl");
    let ordinary = b"{\"type\":\"event_msg\",\"payload\":{\"type\":\"token_count\"}}\n";
    fs::write(&native, ordinary).unwrap();
    store.save(&fixture(&root, &worker, &native)).unwrap();
    drop(store);
    provider_block::observe(&worker).unwrap();
    let cursor = root.join("provider-block/cursor.json");
    let cached = fs::read(&cursor).unwrap();
    provider_block::observe(&worker).unwrap();
    assert_eq!(fs::read(&cursor).unwrap(), cached);
    let row=json!({"type":"event_msg","payload":{"type":"task_complete","error":{"codex_error_info":"misalignment_policy_violation"}}}).to_string();
    let mut append = fs::OpenOptions::new().append(true).open(&native).unwrap();
    append.write_all(row.as_bytes()).unwrap();
    provider_block::observe(&worker).unwrap();
    assert_eq!(
        fs::read(&cursor).unwrap(),
        cached,
        "partial native record remains unread"
    );
    append.write_all(b"\n").unwrap();
    assert!(
        provider_block::observe(&worker)
            .unwrap_err()
            .to_string()
            .starts_with("REVIEW_PROVIDER_BLOCKED")
    );
    let block: Value = source_store::read(&root.join("provider-block.json")).unwrap();
    assert_eq!(block["native"]["offset"], ordinary.len());
    assert_eq!(block["native"]["record"], format!("{row}\n"));
    fs::remove_dir_all(root.parent().unwrap()).unwrap();
}
fn fixture(root: &std::path::Path, worker: &str, native: &std::path::Path) -> State {
    serde_json::from_value(json!({
        "worker":{"provider":"codex","session":worker,"project":root,"pane":null,"socket":null},
        "observer":{"session":"observer-unit","pane":null,"name":"fixture","workspace":root,
            "model":"gpt-6.1-sol","effort":"low","transcript":native},
        "pending":{"id":"review-1-1","purpose":"checkpoint","turn":"t","head":null,"created_ms":1,"native_tokens_before":null},
        "latest":null,"last_verified_turn":null,"checkpoint":null,"deferrals":[],"wakes":0,"native_tokens":0,"stopped":false})).unwrap()
}
