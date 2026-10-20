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

#[test]
fn a_blocked_admission_stops_the_waiting_completion_with_its_reason() {
    // 5r-i on 1a4ef14: admission of the queued run_completion trigger failed
    // with RUN_SHARE_ACL. Nothing retried it, and the run waited with no
    // limit until the 1800 s journey deadline.
    let worker = format!("completion-admission-unit-{}", std::process::id());
    let store = Store::open(&worker).unwrap();
    let directory = store.directory.clone();
    kah_cli::unattended_report::save(&directory,"config.json",&json!({"enabled":true,"provider":"claude","worker":worker,"turn":"t","pid":null,"process_stamp":null})).unwrap();
    let mut key = Key {
        instance: "1000-7".into(),
        commit: "sha".into(),
        user_sequence: 0,
        action_digest: "actions".into(),
        command: "node".into(),
        online: false,
    };
    let receipt = Receipt {
        commit: key.commit.clone(),
        command: key.command.clone(),
        success: true,
        exit_code: Some(0),
        elapsed_ms: 1,
        output: "FUNCTION_OK".into(),
        output_truncated: false,
    };
    let state:State=serde_json::from_value(json!({"worker":{"provider":"claude","session":worker,"project":"/fixture","pane":null,"socket":null},"observer":{"session":null,"pane":null,"name":"observer","workspace":"/fixture","model":"claude-haiku-5-5","effort":"high","transcript":null},"pending":null,"latest":null,"last_verified_turn":null,"checkpoint":null,"deferrals":[],"wakes":0,"native_tokens":0,"stopped":false})).unwrap();
    store.save(&state).unwrap();
    drop(store);
    // The trigger for this delivery is already queued.
    kah_cli::unattended_report::save(
        &directory,
        "completion-checkpoint-request.json",
        &json!({"key":key,"revision":2,"invalidation":null}),
    )
    .unwrap();
    let request =
        |key: &Key| completion_checkpoint::request(&directory, &worker, key, 2, &receipt).unwrap();
    assert!(matches!(request(&key), Status::Pending));
    let block = |at_ms: u64| {
        kah_cli::unattended_report::save(&directory, "admission-blocked.json", &json!({"at_ms":at_ms,
            "error":"RUN_SHARE_ACL stage=worker_prepare: /usr/bin/find: '/w/outbox/build-o48f': Permission denied\n"})).unwrap();
    };
    block(2000);
    // This controller instance already retried admission once at that block.
    // (A missing record starts a real __review-wake, which a unit test avoids.)
    let retried = json!({"instance":key.instance,"block_at_ms":2000});
    kah_cli::unattended_report::save(&directory, "completion-admission-retry.json", &retried)
        .unwrap();
    assert!(matches!(request(&key), Status::Pending), "the retry runs");
    // The retry failed too, so the run stops with the reason.
    block(2500);
    let Status::Blocked(reason) = request(&key) else {
        panic!("a blocked admission must not leave the completion pending");
    };
    assert!(
        reason.contains("RUN_SHARE_ACL stage=worker_prepare"),
        "{reason}"
    );
    assert!(reason.contains("kah resume"), "{reason}");
    // A different instance (after kah resume) is never stopped by the old
    // record: it gets its own retry first.
    key.instance = "3000-7".into();
    let next = json!({"instance":key.instance,"block_at_ms":2500});
    kah_cli::unattended_report::save(&directory, "completion-admission-retry.json", &next).unwrap();
    assert!(matches!(request(&key), Status::Pending));
    fs::remove_dir_all(directory.parent().unwrap()).unwrap();
}
