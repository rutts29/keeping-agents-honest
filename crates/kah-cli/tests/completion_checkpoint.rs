use kah_cli::{
    completion_checkpoint::{self, Status},
    completion_review::Key,
    observer_review::state::{Pending, Purpose, State, Store},
};
use kah_run::verification::Receipt;
use serde_json::json;
use std::fs;

#[test]
fn only_a_matching_passed_checkpoint_unblocks_completion() {
    let worker = format!("completion-checkpoint-unit-{}", std::process::id());
    let store = Store::open(&worker).unwrap();
    let directory = store.directory.clone();
    kah_cli::unattended_report::save(&directory,"config.json",&json!({"enabled":true,"provider":"codex","worker":worker,"turn":"t","pid":null,"process_stamp":null})).unwrap();
    let key = Key {
        instance: "controller".into(),
        commit: "sha".into(),
        user_sequence: 1,
        action_digest: "actions".into(),
        command: "integration".into(),
        online: false,
    };
    let receipt = Receipt {
        commit: key.commit.clone(),
        command: key.command.clone(),
        success: true,
        exit_code: Some(0),
        elapsed_ms: 1,
        output: "verified integration".into(),
        output_truncated: false,
    };
    let mut state:State=serde_json::from_value(json!({"worker":{"provider":"codex","session":worker,"project":"/fixture","pane":null,"socket":null},"observer":{"session":null,"pane":null,"name":"observer","workspace":"/fixture","model":"gpt-6.1-sol","effort":"low","transcript":null},"pending":null,"latest":null,"last_verified_turn":null,"checkpoint":{"head":"sha","intent_revision":2,"passed":true,"completion_key":key},"deferrals":[],"wakes":0,"native_tokens":0,"stopped":false})).unwrap();
    store.save(&state).unwrap();
    drop(store);
    assert!(matches!(
        completion_checkpoint::request(&directory, &worker, &key, 2, &receipt).unwrap(),
        Status::Passed
    ));
    let store = Store::open(&worker).unwrap();
    state.checkpoint.as_mut().unwrap()["passed"] = false.into();
    store.save(&state).unwrap();
    drop(store);
    assert!(matches!(
        completion_checkpoint::request(&directory, &worker, &key, 2, &receipt).unwrap(),
        Status::Failed(_)
    ));
    let store = Store::open(&worker).unwrap();
    state.pending = Some(Pending {
        id: "review-next".into(),
        purpose: Purpose::Checkpoint,
        turn: "t".into(),
        head: Some("sha".into()),
        created_ms: 0,
        native_tokens_before: Some(0),
    });
    store.save(&state).unwrap();
    drop(store);
    assert!(matches!(
        completion_checkpoint::request(&directory, &worker, &key, 2, &receipt).unwrap(),
        Status::Pending
    ));
    let mut wrong = receipt;
    wrong.commit = "other".into();
    assert!(
        completion_checkpoint::request(&directory, &worker, &key, 2, &wrong)
            .unwrap_err()
            .to_string()
            .starts_with("RUN_CHECKPOINT_VERIFICATION")
    );
    fs::remove_dir_all(directory.parent().unwrap()).unwrap();
}
