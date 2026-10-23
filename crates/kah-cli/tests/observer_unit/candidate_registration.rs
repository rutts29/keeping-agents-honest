//! Synthetic controller metadata; no Git/model/transport operations.
use super::*;
use crate::observer_review::state::State;
use std::os::unix::fs::PermissionsExt;

#[test]
fn a_candidate_can_register_in_a_missing_private_index_and_be_looked_up() {
    std::env::var_os("XDG_STATE_HOME").expect("isolated check-tests fixture required");
    let worker = format!("candidate-register-{}", crate::relay_store::nonce());
    let store = Store::open(&worker).unwrap();
    let directory = store.directory.clone();
    let root = directory.join("interactive-gates/retained");
    let head = "3a5c7e9b1d2f4a6c8e0b2d4f6a8c0e2b4d6f8a1c";
    let mut candidate = json!({"base_commit":head,"branch":"kah/run-gate-candidate-1"});
    let git_dir = root.join("candidate/repo.git");
    kah_core::run_store::ensure_private_directory(&git_dir).unwrap();
    candidate["path"] = json!(root.join("candidate/worktree"));
    candidate["git_dir"] = json!(git_dir);
    candidate["admin"] = candidate["git_dir"].clone();
    candidate["source"] = json!(root);
    crate::unattended_report::save(&root, "candidate.json", &candidate).unwrap();
    let state:State=serde_json::from_value(json!({"worker":{"provider":"claude","session":worker,"project":root,"pane":null,"socket":null},
        "observer":{"session":"recorded-observer","pane":null,"name":"recorded-observer","workspace":root,
          "model":"claude-sonnet-5-5","effort":"low","transcript":null},"pending":null,"latest":null,
        "checkpoint":null,"last_verified_turn":null,"deferrals":[],"wakes":0,"native_tokens":0,"stopped":false})).unwrap();
    let index = directory.join("interactive-candidates");
    assert!(!index.exists(), "the private index starts missing");
    source_store::save(
        &index,
        &format!("{head}.json"),
        &json!({"worker":worker,"head":head,
        "project":state.worker.project,"candidate":root.join("candidate.json")}),
    )
    .unwrap();
    assert_eq!(
        fs::metadata(&index).unwrap().permissions().mode() & 0o777,
        0o700
    );
    let loaded = candidate_for(&state, head).unwrap();
    assert_eq!(serde_json::to_value(loaded).unwrap(), candidate);
    drop(store);
    fs::remove_dir_all(directory.parent().unwrap()).unwrap();
}
