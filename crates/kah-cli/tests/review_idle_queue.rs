use kah_cli::observer_review::{
    queue,
    state::{Purpose, State, Store},
};
use serde_json::json;
use std::{
    io::Write,
    process::{Command, Stdio},
};

#[test]
fn an_idle_observer_hook_wakes_a_trigger_left_by_a_busy_store_race() {
    let worker = format!("idle-queue-unit-{}", std::process::id());
    let store = Store::open(&worker).unwrap();
    let root = store.directory.clone();
    let workspace = root.join("observer");
    std::fs::create_dir_all(&workspace).unwrap();
    let state:State=serde_json::from_value(json!({
        "worker":{"provider":"codex","session":worker,"project":workspace,"pane":"fixture","socket":null},
        "observer":{"session":"observer","pane":"fixture","name":"observer","workspace":workspace,
            "model":"gpt-6.1-sol","effort":"low","transcript":null},
        "pending":null,"latest":null,"last_verified_turn":null,"checkpoint":null,
        "deferrals":[],"wakes":0,"native_tokens":0,"stopped":false})).unwrap();
    store.save(&state).unwrap();
    // Invalid snapshot deliberately stops the detached job before any native
    // provider/session can launch. It proves the real host hook dispatches it.
    queue::enqueue(
        &worker,
        Purpose::Checkpoint,
        "t",
        json!({"captured_snapshot":{}}),
    )
    .unwrap();
    drop(store);
    // Detached jobs must use this build, never the operator's installed binary.
    let home = root.join("test-home");
    std::fs::create_dir_all(home.join(".local/bin")).unwrap();
    std::fs::copy(env!("CARGO_BIN_EXE_kah"), home.join(".local/bin/kah")).unwrap();
    let mut child = Command::new(env!("CARGO_BIN_EXE_kah"))
        .args(["__review-hook", &worker])
        .env("HERDR_PANE_ID", "fixture")
        .env_remove("HERDR_SOCKET_PATH")
        .env("HOME", &home)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(
            &serde_json::to_vec(&json!({
        "hook_event_name":"Stop","session_id":"observer","cwd":workspace}))
            .unwrap(),
        )
        .unwrap();
    assert!(child.wait().unwrap().success());
    // The native invocation now persists and returns; dispatch is detached.
    // Without a pending request, a real failure is archived
    // rather than becoming an unbound gap on the next admitted request.
    let failure = archived_failure(&root);
    assert_eq!(failure["current_request"], serde_json::Value::Null);
    assert!(failure["gap"]["error"].as_str().unwrap().contains("stage="));
    assert!(!root.join("gap.json").exists());
    assert!(
        root.join("job-__review-wake.log").exists(),
        "idle hook must wake the queued trigger"
    );
    assert_eq!(
        queue::entries(&worker).unwrap().len(),
        1,
        "invalid evidence must remain queued"
    );
    std::fs::remove_dir_all(root.parent().unwrap()).unwrap();
}

fn archived_failure(root: &std::path::Path) -> serde_json::Value {
    loop {
        if let Ok(files) = std::fs::read_dir(root.join("retired-gaps")) {
            for file in files.flatten() {
                // Atomic writers publish JSON by rename; temporary files are not receipts.
                if file.path().extension().is_some_and(|v| v == "json") {
                    return serde_json::from_slice(&std::fs::read(file.path()).unwrap()).unwrap();
                }
            }
        }
        std::thread::yield_now();
    }
}
