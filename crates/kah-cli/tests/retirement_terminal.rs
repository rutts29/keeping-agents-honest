//! A report-absent drift lane ends as a terminal not_checked gap, with no
//! judgment; a modern submitted report prevents retirement.
use kah_cli::observer_review::{
    consume::retired,
    state::{State, Store},
};
use serde_json::{Value, json};
use std::{fs, path::Path};

const REQUEST: &str = "review-1700000700000-701";
const HEAD: &str = "c3d4e5f60718293a4b5c6d7e8f90112233445566";

fn state(root: &Path) -> Value {
    json!({"worker":{"provider":"codex","session":"retire-worker","project":root.join("project"),"pane":null,"socket":null},
        "observer":{"session":"observer-one","pane":null,"name":"fixture","workspace":root.join("workspace"),
            "model":"gpt-6.1-sol","effort":"low","transcript":null},
        "pending":{"id":REQUEST,"purpose":"drift","turn":"turn-5","head":HEAD,"created_ms":1,"native_tokens_before":null},
        "latest":null,"last_verified_turn":"turn-4","checkpoint":null,"deferrals":[],
        "wakes":3,"native_tokens":0,"stopped":false})
}
/// A drift lane with its request, a queued drift wake and a queued checkpoint wake.
fn fixture(root: &Path) -> Store {
    kah_core::run_store::ensure_private_directory(root).unwrap();
    fs::create_dir_all(root.join("workspace/outbox")).unwrap();
    kah_cli::unattended_report::save(root, "state.json", &state(root)).unwrap();
    let request = json!({"id":REQUEST,"purpose":"drift","turn":"turn-5","head":HEAD});
    let requests = root.join("requests");
    kah_core::run_store::ensure_private_directory(&requests).unwrap();
    kah_cli::unattended_report::save(&requests, &format!("{REQUEST}.json"), &request).unwrap();
    let queued = root.join("queued");
    kah_core::run_store::ensure_private_directory(&queued).unwrap();
    for (name, purpose) in [
        ("0001-drift.json", "drift"),
        ("0002-gate.json", "checkpoint"),
    ] {
        let wake = json!({"purpose":purpose,"turn":"turn-6","trigger":{"events":[]}});
        kah_cli::unattended_report::save(&queued, name, &wake).unwrap();
    }
    Store::delivery_at(root, true).unwrap()
}

#[test]
fn a_report_absent_drift_lane_ends_in_a_terminal_gap_once() {
    let root = std::env::temp_dir().join(format!("kah-retire-{}", std::process::id()));
    let store = fixture(&root);
    let request_before = fs::read(root.join(format!("requests/{REQUEST}.json"))).unwrap();
    assert_eq!(retired::queued_at(&root).unwrap(), 1);
    assert!(!root.join("queued/0001-drift.json").exists());
    assert!(root.join("queued/0002-gate.json").exists());
    let gap: Value = serde_json::from_slice(
        &fs::read(root.join("retired-tripwire/queued/gap-0001-drift.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(gap["status"], "not_checked");
    assert_eq!(gap["gap"]["code"], "retired_tripwire");
    assert!(retired::at(&store).unwrap());
    let after = store.load().unwrap().unwrap();
    assert!(after.pending.is_none());
    let latest = after.latest.as_ref().unwrap();
    assert_eq!(latest["status"], "not_checked");
    assert_eq!(latest["gap"]["code"], "retired_tripwire");
    assert_eq!(latest["real_jev_calls"], 0);
    assert_eq!(after.last_verified_turn.as_deref(), Some("turn-4"));
    let request_after = fs::read(root.join(format!("requests/{REQUEST}.json"))).unwrap();
    assert_eq!(request_after, request_before);
    // Idempotent: nothing left to retire.
    assert_eq!(retired::queued_at(&root).unwrap(), 0);
    assert!(!retired::at(&store).unwrap());
    drop(store);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn a_modern_submitted_report_prevents_retirement_and_keeps_state_bytes() {
    let root = std::env::temp_dir().join(format!("kah-retire-modern-{}", std::process::id()));
    let store = fixture(&root);
    let state: State = serde_json::from_value(state(&root)).unwrap();
    let outbox = root.join("workspace/outbox");
    let name = format!("submitted-{REQUEST}.json");
    let modern = json!({"decision_units":{"version":12,"requirements":[]}});
    kah_cli::unattended_report::save(&outbox, &name, &modern).unwrap();
    let report = outbox.join(name);
    store.save(&state).unwrap();
    let before = fs::read(root.join("state.json")).unwrap();
    assert!(!retired::at(&store).unwrap());
    assert_eq!(fs::read(root.join("state.json")).unwrap(), before);
    fs::remove_file(&report).unwrap();
    assert!(retired::at(&store).unwrap());
    assert!(store.load().unwrap().unwrap().pending.is_none());
    drop(store);
    fs::remove_dir_all(root).unwrap();
}
