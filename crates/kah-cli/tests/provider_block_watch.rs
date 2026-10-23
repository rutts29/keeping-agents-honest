//! Native terminal error record; no agent, provider call or keyboard operation.
use kah_cli::observer_review::{
    liveness, source_store,
    state::{State, Store},
};
use serde_json::{Value, json};
use std::fs;

#[test]
fn native_policy_error_is_terminal_and_preserves_pending() {
    std::env::var_os("XDG_STATE_HOME").expect("private check-tests state required");
    let worker = format!("provider-block-{}", kah_cli::relay_store::nonce());
    let store = Store::open(&worker).unwrap();
    let root = store.directory.clone();
    let state = fixture(&root, &worker);
    store.save(&state).unwrap();
    drop(store);
    let result = liveness::run(&worker).unwrap_err().to_string();
    assert!(result.starts_with("REVIEW_PROVIDER_BLOCKED"), "{result}");
    let gap: Value = source_store::read(&root.join("gap.json")).unwrap();
    assert_eq!(gap["status"], "provider_blocked");
    assert_eq!(gap["request"], "review-1-1");
    assert_eq!(gap["native"]["code"], "misalignment_policy_violation");
    assert_eq!(gap["native"]["offset"], 0);
    assert_eq!(gap["native"]["sha256"].as_str().unwrap().len(), 64);
    assert_eq!(
        Store::read(&worker).unwrap().unwrap().pending.unwrap().id,
        "review-1-1"
    );
    assert!(root.parent().unwrap().join("observer-paused").is_file());
    assert!(!root.join("redelivery.json").exists());
    let bytes = fs::read(root.join("provider-block.json")).unwrap();
    assert!(
        liveness::run(&worker)
            .unwrap_err()
            .to_string()
            .starts_with("REVIEW_PROVIDER_BLOCKED")
    );
    assert_eq!(fs::read(root.join("provider-block.json")).unwrap(), bytes);
    assert_eq!(
        liveness::notice(&worker).unwrap().unwrap()["status"],
        "provider_blocked"
    );
    assert!(
        liveness::notice(&worker).unwrap().is_none(),
        "one hook notice per unchanged native block"
    );
    fs::remove_dir_all(root.parent().unwrap()).unwrap();
}
fn fixture(root: &std::path::Path, worker: &str) -> State {
    let transcript = root.join("native.jsonl");
    // One authored terminal record at byte zero, as a Codex turn ends it.
    let record = json!({"timestamp":"2026-01-01T00:00:00.000Z","type":"event_msg","payload":{
        "type":"task_complete","turn_id":"turn-block-1","last_agent_message":null,
        "error":{"message":"The provider stopped this turn under its usage policy.",
            "codex_error_info":"misalignment_policy_violation"}}});
    fs::write(&transcript, format!("{record}\n")).unwrap();
    serde_json::from_value(json!({
        "worker":{"provider":"codex","session":worker,"project":root,"pane":null,"socket":null},
        "observer":{"session":"observer-unit","pane":null,"name":"fixture","workspace":root,
            "model":"gpt-6.1-sol","effort":"low","transcript":transcript},
        "pending":{"id":"review-1-1","purpose":"checkpoint","turn":"t","head":null,"created_ms":1,"native_tokens_before":null},
        "latest":null,"last_verified_turn":null,"checkpoint":null,"deferrals":[],"wakes":0,"native_tokens":0,"stopped":false})).unwrap()
}
