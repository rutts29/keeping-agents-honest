use kah_cli::observer_review::state::{self, Store};
use serde_json::json;
#[test]
fn admission_control_does_not_load_unneeded_evidence_payloads() {
    let worker = format!("admission-control-{}", kah_cli::relay_store::nonce());
    let store = Store::open(&worker).unwrap();
    let value = json!({"worker":{"provider":"codex","session":worker,"project":"/tmp/fixture","pane":null,"socket":null},"observer":{"session":null,"pane":null,"name":"fixture","workspace":"/tmp/fixture","model":"gpt-6.1-sol","effort":"low","transcript":null},"pending":null,"stopped":false,"latest":{"$kah_source":"f".repeat(64),"encoding":"utf8"},"deferrals":[],"last_verified_turn":null,"checkpoint":null,"wakes":0,"native_tokens":0});
    kah_cli::unattended_report::save(&store.directory, "state.json", &value).unwrap();
    let control = state::read_control(&worker).unwrap().unwrap();
    assert!(control.pending.is_none());
    assert_eq!(control.worker.session, worker);
    assert!(!control.stopped);
    assert!(
        Store::read(&worker).is_err(),
        "full evidence readers still verify their original sources"
    );
    let path = store.directory.parent().unwrap().to_owned();
    drop(store);
    std::fs::remove_dir_all(path).unwrap();
}

#[test]
fn an_existing_admission_worker_coalesces_later_wakes_without_loading_state() {
    let worker = format!("admission-owner-{}", kah_cli::relay_store::nonce());
    let store = Store::open(&worker).unwrap();
    let owner = Store::delivery_at(&store.directory.join("admission-worker"), false).unwrap();
    let result = kah_cli::observer_review::queue::drain(&worker).unwrap();
    assert_eq!(result["reason"], "admission_inflight");
    assert!(
        !store.directory.join("state.json").exists(),
        "coalescing must precede state/evidence loading"
    );
    drop(owner);
    let path = store.directory.parent().unwrap().to_owned();
    drop(store);
    std::fs::remove_dir_all(path).unwrap();
}
