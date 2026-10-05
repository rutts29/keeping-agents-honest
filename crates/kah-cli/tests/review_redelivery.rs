use kah_cli::{
    observer_review::{
        runtime,
        state::{Pending, Purpose, Request, State, Store},
    },
    relay_store::Endpoint,
};
use kah_observer::session;
use serde_json::json;
use std::{
    fs,
    path::{Path, PathBuf},
    time::Duration,
};

#[test]
fn failed_native_redelivery_retains_the_pending_review_and_visible_gap() {
    let worker = format!("redelivery-unit-{}", std::process::id());
    let home = std::env::temp_dir().join(&worker);
    let mut state = setup(&worker, &home);
    let request = Request {
        id: "review-retry-unit".into(),
        worker: worker.clone(),
        purpose: Purpose::Drift,
        turn: "turn".into(),
        head: None,
        intent: json!({"goal":"Retain this review"}),
        trigger: json!({}),
        documents: vec![],
        created_ms: 0,
    };
    state.pending = Some(Pending::from(&request));
    state.observer.pane = Some("unreachable-observer".into());
    let store = Store::open(&worker).unwrap();
    store.save(&state).unwrap();
    let requests = store.directory.join("requests");
    kah_core::run_store::ensure_private_directory(&requests).unwrap();
    kah_cli::unattended_report::save(
        &requests,
        "review-retry-unit.json",
        &serde_json::to_value(&request).unwrap(),
    )
    .unwrap();
    let gap = json!({"error":"REVIEW_REPORT_MISSING stage=observer","at_ms":1});
    kah_cli::unattended_report::save(&store.directory, "gap.json", &gap).unwrap();
    let directory = store.directory.clone();
    drop(store);
    assert!(
        runtime::retry_pending(&worker)
            .unwrap_err()
            .to_string()
            .contains("REVIEW_HERDR_REQUIRED")
    );
    assert_eq!(
        Store::read(&worker).unwrap().unwrap().pending.unwrap().id,
        request.id
    );
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&fs::read(directory.join("gap.json")).unwrap())
            .unwrap(),
        gap
    );
    fs::remove_dir_all(directory.parent().unwrap()).unwrap();
}

#[test]
fn redelivery_refreshes_old_helper_and_schema_without_changing_request_sources() {
    let worker = format!("redelivery-upgrade-unit-{}", std::process::id());
    // CAS sharing uses hard links; keep this workspace on the state filesystem.
    let home = kah_core::run_store::RunStore::default_state_root()
        .unwrap()
        .join(&worker);
    let state = setup(&worker, &home);
    fs::create_dir_all(home.join(".local/bin")).unwrap();
    fs::copy(env!("CARGO_BIN_EXE_kah"), home.join(".local/bin/kah")).unwrap();
    fs::create_dir_all(state.observer.workspace.join("bin")).unwrap();
    fs::write(state.observer.workspace.join("bin/kah"), "old helper").unwrap();
    let request = Request {
        id: "review-upgrade-unit".into(),
        worker: worker.clone(),
        purpose: Purpose::Drift,
        turn: "original".into(),
        head: None,
        intent: json!({"goal":"Original whole user words."}),
        trigger: json!({}),
        documents: vec![],
        created_ms: 1,
    };
    let original = serde_json::to_value(&request).unwrap();
    let store = Store::open(&worker).unwrap();
    let prompt =
        kah_cli::observer_review::redelivery::prompt(&home, &store, &state, &request, &request.id)
            .unwrap();
    assert!(prompt.contains("applicability first"));
    assert_ne!(
        fs::read(state.observer.workspace.join("bin/kah")).unwrap(),
        b"old helper"
    );
    let input = state.observer.workspace.join("inputs").join(&request.id);
    let restored: Request =
        kah_cli::observer_review::source_store::read(&input.join("request.json")).unwrap();
    assert_eq!(serde_json::to_value(restored).unwrap(), original);
    let schema: serde_json::Value =
        serde_json::from_slice(&fs::read(input.join("schema.json")).unwrap()).unwrap();
    assert!(
        serde_json::to_string(&schema)
            .unwrap()
            .contains("applicability")
    );
    let directory = store.directory.clone();
    drop(store);
    fs::remove_dir_all(directory.parent().unwrap()).unwrap();
    fs::remove_dir_all(home).unwrap();
}
fn setup(worker: &str, home: &Path) -> State {
    let base = kah_core::run_store::RunStore::default_state_root().unwrap();
    let binding = session::Binding {
        session: worker.into(),
        project: home.into(),
        codex_home: home.join("codex"),
        native: PathBuf::from("/nonexistent/native"),
        model: "gpt-6.1-sol".into(),
        effort: "low".into(),
        timeout_secs: 90,
        judge: Default::default(),
        jev_model: "jev-latest".into(),
    };
    let mut primary = session::Store::open(&base, worker, Duration::ZERO).unwrap();
    primary.save(&session::State::new(binding)).unwrap();
    drop(primary);
    State::new(
        Endpoint {
            provider: "codex".into(),
            session: worker.into(),
            project: home.into(),
            pane: Some("unreachable-worker".into()),
            socket: None,
        },
        home,
    )
    .unwrap()
}

#[test]
fn repeated_failures_are_visible_without_limiting_or_dropping_attempts() {
    use kah_cli::observer_review::redelivery;
    let worker = format!("redelivery-notices-{}", std::process::id());
    let store = Store::open(&worker).unwrap();
    let error = "REVIEW_REPORT_MISSING stage=observer";
    let first = redelivery::begin(&store, "review-repeat", error).unwrap();
    redelivery::outcome(&store, "review-repeat", first, None).unwrap();
    assert!(redelivery::take(&store).unwrap().is_none());
    for number in 2..=5 {
        assert_eq!(
            redelivery::begin(&store, "review-repeat", error).unwrap(),
            number
        );
        let notice = redelivery::take(&store).unwrap().unwrap();
        let text = notice["systemMessage"].as_str().unwrap();
        assert!(text.contains("review-repeat") && text.contains(error));
        redelivery::outcome(&store, "review-repeat", number, Some(error.into())).unwrap();
    }
    redelivery::completed(&store, "review-repeat").unwrap();
    let saved: serde_json::Value =
        serde_json::from_slice(&fs::read(store.directory.join("redelivery.json")).unwrap())
            .unwrap();
    assert_eq!(
        saved["requests"]["review-repeat"]["attempts"]
            .as_array()
            .unwrap()
            .len(),
        5
    );
    assert_eq!(
        saved["requests"]["review-repeat"]["consecutive_failures"],
        0
    );
    assert_eq!(
        redelivery::begin(&store, "review-repeat", error).unwrap(),
        6
    );
    assert!(redelivery::take(&store).unwrap().is_none());
    let directory = store.directory.clone();
    drop(store);
    fs::remove_dir_all(directory.parent().unwrap()).unwrap();
}
