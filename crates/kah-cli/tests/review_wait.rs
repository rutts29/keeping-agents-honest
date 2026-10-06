use kah_cli::observer_review::state::Store;
use std::{fs, sync::mpsc, thread};

#[test]
fn detached_review_waits_for_the_lock_owner_instead_of_losing_the_event() {
    let worker = format!("review-wait-unit-{}", std::process::id());
    let store = Store::open(&worker).unwrap();
    let directory = store.directory.clone();
    let (started_tx, started_rx) = mpsc::channel();
    let (done_tx, done_rx) = mpsc::channel();
    let waiter = thread::spawn(move || {
        started_tx.send(()).unwrap();
        let store = Store::open_wait(&worker).unwrap();
        kah_cli::unattended_report::save(
            &store.directory,
            "processed.json",
            &serde_json::json!({"whole":"event"}),
        )
        .unwrap();
        done_tx.send(()).unwrap();
    });
    started_rx.recv().unwrap();
    assert!(done_rx.try_recv().is_err());
    assert!(!directory.join("processed.json").exists());
    drop(store);
    done_rx.recv().unwrap();
    waiter.join().unwrap();
    assert!(directory.join("processed.json").exists());
    fs::remove_dir_all(directory.parent().unwrap()).unwrap();
}

#[test]
fn judgment_recording_waits_for_the_primary_ledger_owner() {
    use kah_observer::session::Store as Primary;
    let base = std::env::temp_dir().join(format!("kah-primary-wait-{}", std::process::id()));
    let owner = Primary::open(&base, "worker", std::time::Duration::ZERO).unwrap();
    let copy = base.clone();
    let waiter = thread::spawn(move || {
        let mut primary = Primary::open_wait(&copy, "worker").unwrap();
        primary
            .append(&serde_json::json!({"kind":"judgment","kept":true}))
            .unwrap();
    });
    drop(owner);
    waiter.join().unwrap();
    assert!(
        fs::read_to_string(Primary::directory(&base, "worker").join("observations.jsonl"))
            .unwrap()
            .contains("judgment")
    );
    fs::remove_dir_all(base).unwrap();
}

#[test]
fn deferred_native_binding_keeps_original_identity_and_scope_after_contention() {
    use kah_cli::observer_review::{hook_queue, state::State};
    use serde_json::json;
    let worker = format!("hook-bind-wait-unit-{}", std::process::id());
    let workspace = std::env::temp_dir().join(&worker);
    fs::create_dir_all(&workspace).unwrap();
    let state: State = fixture_state(&worker, &workspace);
    let store = Store::open(&worker).unwrap();
    store.save(&state).unwrap();
    let directory = store.directory.clone();
    let events = directory.join("hook-events");
    kah_core::run_store::ensure_private_directory(&events).unwrap();
    let event = hook_queue::Event {
        input: json!({"hook_event_name":"SessionEnd","session_id":"native-session","cwd":workspace,"transcript_path":"/native/transcript.jsonl"}),
        pid: Some(std::process::id()),
        pane: Some("observer-pane".into()),
        socket: Some("/private/herdr.sock".into()),
        validated_pane: None,
        context: None,
        transcript_end: None,
    };
    kah_cli::unattended_report::save(&events, "first.json", &serde_json::to_value(event).unwrap())
        .unwrap();
    let copy = worker.clone();
    let waiter = thread::spawn(move || hook_queue::run(&copy).unwrap());
    assert!(events.join("first.json").exists());
    drop(store);
    waiter.join().unwrap();
    let result = Store::read(&worker).unwrap().unwrap();
    assert_eq!(result.observer.session.as_deref(), Some("native-session"));
    assert_eq!(result.observer.pid, Some(std::process::id()));
    assert!(result.stopped);
    assert!(!events.join("first.json").exists());
    fs::remove_dir_all(directory.parent().unwrap()).unwrap();
    fs::remove_dir_all(workspace).unwrap();
}

fn fixture_state(
    worker: &str,
    workspace: &std::path::Path,
) -> kah_cli::observer_review::state::State {
    serde_json::from_value(serde_json::json!({
        "worker":{"provider":"codex","session":worker,"project":workspace,"pane":"worker-pane","socket":"/private/herdr.sock"},
        "observer":{"session":null,"pane":"observer-pane","name":"observer","workspace":workspace,"model":"gpt-6.1-sol","effort":"low","transcript":null,"pid":null,"process_stamp":null},
        "pending":null,"latest":null,"last_verified_turn":null,"checkpoint":null,"deferrals":[],"wakes":0,"native_tokens":0,"stopped":false
    })).unwrap()
}

#[test]
fn idle_without_a_preparation_acknowledgement_keeps_the_request_and_shows_a_named_gap() {
    use kah_cli::observer_review::{
        hook_queue,
        state::{Pending, Purpose},
    };
    use serde_json::json;
    let worker = format!("missing-selection-unit-{}", std::process::id());
    let workspace = std::env::temp_dir().join(&worker);
    fs::create_dir_all(&workspace).unwrap();
    let mut state = fixture_state(&worker, &workspace);
    state.pending = Some(Pending {
        id: "review-selection".into(),
        purpose: Purpose::Drift,
        turn: "t".into(),
        head: None,
        created_ms: 0,
        native_tokens_before: Some(0),
    });
    let store = Store::open(&worker).unwrap();
    store.save(&state).unwrap();
    let directory = store.directory.clone();
    kah_cli::unattended_report::save(&directory, "preparation-review-selection.json", &json!({}))
        .unwrap();
    let events = directory.join("hook-events");
    kah_core::run_store::ensure_private_directory(&events).unwrap();
    let event = hook_queue::Event {
        input: json!({"hook_event_name":"Stop","session_id":"native-session","cwd":workspace}),
        pid: Some(std::process::id()),
        pane: Some("observer-pane".into()),
        socket: Some("/private/herdr.sock".into()),
        validated_pane: None,
        context: None,
        transcript_end: None,
    };
    kah_cli::unattended_report::save(&events, "stop.json", &serde_json::to_value(event).unwrap())
        .unwrap();
    drop(store);
    hook_queue::run(&worker).unwrap();
    assert_eq!(
        Store::read(&worker).unwrap().unwrap().pending.unwrap().id,
        "review-selection"
    );
    let gap = fs::read_to_string(directory.join("gap.json")).unwrap();
    assert!(gap.contains("REVIEW_PREPARATION_MISSING"));
    assert!(gap.contains("review-selection"));
    fs::remove_dir_all(directory.parent().unwrap()).unwrap();
    fs::remove_dir_all(workspace).unwrap();
}

#[test]
fn a_deferred_stop_for_the_old_request_cannot_fault_the_new_review() {
    use kah_cli::observer_review::{
        hook_queue,
        request_context::Context,
        state::{Pending, Purpose},
    };
    use serde_json::json;
    let worker = format!("old-stop-unit-{}", std::process::id());
    let workspace = std::env::temp_dir().join(&worker);
    fs::create_dir_all(&workspace).unwrap();
    let mut state = fixture_state(&worker, &workspace);
    state.pending = Some(Pending {
        id: "review-new".into(),
        purpose: Purpose::Drift,
        turn: "t".into(),
        head: None,
        created_ms: 0,
        native_tokens_before: Some(0),
    });
    let store = Store::open(&worker).unwrap();
    store.save(&state).unwrap();
    let directory = store.directory.clone();
    let events = directory.join("hook-events");
    kah_core::run_store::ensure_private_directory(&events).unwrap();
    let event = hook_queue::Event {
        input: json!({"hook_event_name":"Stop","session_id":"native-session","cwd":workspace}),
        pid: Some(std::process::id()),
        pane: Some("observer-pane".into()),
        socket: Some("/private/herdr.sock".into()),
        validated_pane: None,
        transcript_end: None,
        context: Some(Context {
            request: "review-old".into(),
            preparation: false,
        }),
    };
    kah_cli::unattended_report::save(
        &events,
        "old-stop.json",
        &serde_json::to_value(event).unwrap(),
    )
    .unwrap();
    drop(store);
    hook_queue::run(&worker).unwrap();
    assert!(!directory.join("gap.json").exists());
    assert_eq!(
        Store::read(&worker).unwrap().unwrap().pending.unwrap().id,
        "review-new"
    );
    assert!(!events.join("old-stop.json").exists());
    fs::remove_dir_all(directory.parent().unwrap()).unwrap();
    fs::remove_dir_all(workspace).unwrap();
}
