use kah_cli::observer_review::{
    hook_queue,
    state::{State, Store},
};
use serde_json::{Value, json};
use std::fs;

fn scenario(
    suffix: &str,
    socket: &str,
    session: &str,
    validated: bool,
) -> (String, std::path::PathBuf) {
    let worker = format!("retired-hook-{suffix}-{}", std::process::id());
    let workspace = std::env::temp_dir().join(&worker);
    fs::create_dir(&workspace).unwrap();
    let state: State = serde_json::from_value(json!({
        "worker":{"provider":"claude","session":worker,"project":workspace,"pane":"worker","socket":"/private/herdr.sock"},
        "observer":{"session":"same-native-session","pane":"new-pane","name":"observer","workspace":workspace,"model":"claude-sonnet-5-5","effort":"low","transcript":null,"pid":1234,"process_stamp":null},
        "pending":null,"latest":null,"last_verified_turn":null,"checkpoint":null,"deferrals":[],"wakes":1,"native_tokens":0,"stopped":false
    })).unwrap();
    let store = Store::open(&worker).unwrap();
    store.save(&state).unwrap();
    let queued = store.directory.join("hook-events");
    kah_core::run_store::ensure_private_directory(&queued).unwrap();
    kah_cli::unattended_report::save(
        &queued,
        "old-end.json",
        &json!({
            "input":{"cwd":workspace,"hook_event_name":"SessionEnd","session_id":session},
            "pid":111,"pane":"old-pane","socket":socket,"context":null,
            "validated_pane":if validated {Some("old-pane")} else {None}
        }),
    )
    .unwrap();
    (worker, workspace)
}
fn cleanup(worker: &str, workspace: std::path::PathBuf) {
    fs::remove_dir_all(Store::directory(worker).unwrap().parent().unwrap()).unwrap();
    fs::remove_dir_all(workspace).unwrap();
}
#[test]
fn already_validated_old_session_end_is_retained_without_stopping_resumed_observer() {
    let (worker, workspace) = scenario(
        "old-end",
        "/private/herdr.sock",
        "same-native-session",
        true,
    );
    hook_queue::run(&worker).unwrap();
    let state = Store::read(&worker).unwrap().unwrap();
    assert_eq!(state.observer.pane.as_deref(), Some("new-pane"));
    assert_eq!(state.observer.pid, Some(1234));
    assert!(!state.stopped);
    let root = Store::directory(&worker).unwrap();
    let retained: Value = serde_json::from_slice(
        &fs::read(
            fs::read_dir(root.join("retired-hook-events"))
                .unwrap()
                .next()
                .unwrap()
                .unwrap()
                .path(),
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(retained["input"]["hook_event_name"], "SessionEnd");
    assert_eq!(retained["pane"], "old-pane");
    assert!(!root.join("hook-events/old-end.json").exists());
    cleanup(&worker, workspace);
}
#[test]
fn a_prior_validation_cannot_authorize_a_different_socket_or_session() {
    for (suffix, socket, session, validated) in [
        ("socket", "/other/herdr.sock", "same-native-session", true),
        (
            "session",
            "/private/herdr.sock",
            "different-native-session",
            true,
        ),
    ] {
        let (worker, workspace) = scenario(suffix, socket, session, validated);
        assert!(
            hook_queue::run(&worker)
                .unwrap_err()
                .to_string()
                .starts_with("REVIEW_HOOK_SCOPE")
        );
        assert_eq!(
            Store::read(&worker)
                .unwrap()
                .unwrap()
                .observer
                .pane
                .as_deref(),
            Some("new-pane")
        );
        cleanup(&worker, workspace);
    }
}

#[test]
fn validated_stop_for_a_completed_request_cannot_fault_the_resumed_request() {
    use kah_cli::observer_review::state::{Pending, Purpose};
    let (worker, workspace) = scenario(
        "old-stop",
        "/private/herdr.sock",
        "same-native-session",
        true,
    );
    let store = Store::open(&worker).unwrap();
    let mut state = store.load().unwrap().unwrap();
    state.pending = Some(Pending {
        id: "review-new".into(),
        purpose: Purpose::Drift,
        turn: "new".into(),
        head: None,
        created_ms: 0,
        native_tokens_before: None,
    });
    store.save(&state).unwrap();
    let event_path = store.directory.join("hook-events/old-end.json");
    let mut event: Value = serde_json::from_slice(&fs::read(&event_path).unwrap()).unwrap();
    event["input"]["hook_event_name"] = "Stop".into();
    event["context"] = json!({"request":"review-old","preparation":false});
    kah_cli::unattended_report::save(event_path.parent().unwrap(), "old-end.json", &event).unwrap();
    drop(store);
    hook_queue::run(&worker).unwrap();
    let current = Store::read(&worker).unwrap().unwrap();
    assert_eq!(current.pending.unwrap().id, "review-new");
    assert_eq!(current.observer.pane.as_deref(), Some("new-pane"));
    assert!(!Store::directory(&worker).unwrap().join("gap.json").exists());
    assert_eq!(
        fs::read_dir(
            Store::directory(&worker)
                .unwrap()
                .join("retired-hook-events")
        )
        .unwrap()
        .count(),
        1
    );
    cleanup(&worker, workspace);
}

#[test]
fn validated_current_request_hook_is_accepted_without_replacing_live_transport() {
    use kah_cli::observer_review::state::{Pending, Purpose};
    let (worker, workspace) = scenario(
        "current-stop",
        "/private/herdr.sock",
        "same-native-session",
        true,
    );
    let store = Store::open(&worker).unwrap();
    let mut state = store.load().unwrap().unwrap();
    state.pending = Some(Pending {
        id: "review-new".into(),
        purpose: Purpose::Drift,
        turn: "new".into(),
        head: None,
        created_ms: 0,
        native_tokens_before: None,
    });
    store.save(&state).unwrap();
    let path = store.directory.join("hook-events/old-end.json");
    let mut event: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    event["input"]["hook_event_name"] = "Stop".into();
    event["context"] = json!({"request":"review-new","preparation":false});
    kah_cli::unattended_report::save(path.parent().unwrap(), "old-end.json", &event).unwrap();
    drop(store);
    hook_queue::run(&worker).unwrap();
    assert_eq!(
        Store::read(&worker).unwrap().unwrap().pending.unwrap().id,
        "review-new"
    );
    cleanup(&worker, workspace);
}

#[test]
fn every_validated_hook_type_is_accepted_after_same_session_pane_change() {
    for kind in [
        "SessionStart",
        "UserPromptSubmit",
        "PreToolUse",
        "PostToolUse",
        "PostToolUseFailure",
        "Stop",
        "SessionEnd",
    ] {
        let (worker, workspace) =
            scenario(kind, "/private/herdr.sock", "same-native-session", true);
        let root = Store::directory(&worker).unwrap();
        let path = root.join("hook-events/old-end.json");
        let mut event: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        event["input"]["hook_event_name"] = kind.into();
        kah_cli::unattended_report::save(path.parent().unwrap(), "old-end.json", &event).unwrap();
        hook_queue::run(&worker).unwrap();
        let current = Store::read(&worker).unwrap().unwrap();
        assert_eq!(current.observer.pane.as_deref(), Some("new-pane"), "{kind}");
        assert_eq!(current.observer.pid, Some(1234), "{kind}");
        assert!(!current.stopped, "{kind}");
        assert_eq!(
            fs::read_dir(root.join("retired-hook-events"))
                .unwrap()
                .count(),
            1,
            "{kind}"
        );
        assert!(!path.exists(), "{kind}");
        cleanup(&worker, workspace);
    }
}

#[test]
fn matching_native_session_cannot_cross_a_worker_workspace() {
    let (worker, workspace) = scenario(
        "wrong-workspace",
        "/private/herdr.sock",
        "same-native-session",
        true,
    );
    let root = Store::directory(&worker).unwrap();
    let path = root.join("hook-events/old-end.json");
    let mut event: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    event["input"]["cwd"] = std::env::temp_dir().to_string_lossy().into_owned().into();
    kah_cli::unattended_report::save(path.parent().unwrap(), "old-end.json", &event).unwrap();
    assert!(
        hook_queue::run(&worker)
            .unwrap_err()
            .to_string()
            .starts_with("REVIEW_HOOK_SCOPE")
    );
    cleanup(&worker, workspace);
}
