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
    failed_redelivery(
        kah_cli::observer_review::failure::Origin::Delivery,
        "REVIEW_REPORT_MISSING stage=observer",
        "missing",
    );
}

#[test]
fn invalid_context_selection_retries_without_accepting_or_discarding_it() {
    failed_redelivery(
        kah_cli::observer_review::failure::Origin::SelectionValidation,
        "CONTEXT_UNKNOWN_ITEM stage=context_selection",
        "selection",
    );
}

#[test]
fn historical_agent_wait_timeout_retries_after_later_worker_events() {
    failed_redelivery(
        kah_cli::observer_review::failure::Origin::Delivery,
        "RUN_HERDR_COMMAND stage=pane: agent wait: {\"error\":{\"code\":\"timeout\",\"message\":\"timed out waiting for agent status\"},\"id\":\"cli:agent:wait\"}\n",
        "wait-timeout",
    );
}

fn failed_redelivery(origin: kah_cli::observer_review::failure::Origin, error: &str, suffix: &str) {
    let worker = format!("redelivery-unit-{suffix}-{}", std::process::id());
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
    let gap = json!({"error":error,"at_ms":1,"origin":origin});
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
    // Real native-lifecycle-1 failure: redelivery republishes an existing input.
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
fn context_redelivery_does_not_republish_unrelated_report_documents() {
    use kah_cli::observer_review::{preparation, redelivery};
    let worker = format!("redelivery-context-only-{}", kah_cli::relay_store::nonce());
    let home = kah_core::run_store::RunStore::default_state_root()
        .unwrap()
        .join(&worker);
    let mut state = setup(&worker, &home);
    fs::create_dir_all(home.join(".local/bin")).unwrap();
    fs::copy(env!("CARGO_BIN_EXE_kah"), home.join(".local/bin/kah")).unwrap();
    let request = Request {
        id: "review-context-only".into(),
        worker: worker.clone(),
        purpose: Purpose::Drift,
        turn: "original".into(),
        head: None,
        intent: json!({"goal":"Original whole authority."}),
        trigger: json!({}),
        documents: vec![kah_observer::review_evidence::Document::new(
            "not/needed-for-selection",
            "evidence",
            "fixture",
            "Whole retained report source.",
        )],
        created_ms: 1,
    };
    state.pending = Some(Pending::from(&request));
    let store = Store::open(&worker).unwrap();
    store.save(&state).unwrap();
    let input = state.observer.workspace.join("inputs").join(&request.id);
    fs::create_dir_all(&input).unwrap();
    fs::write(input.join("request.json"), "retained report packet").unwrap();
    let decision = "review-context-only-v8-one";
    let mut local = request.clone();
    local.id = decision.into();
    let original = json!({"intent":request.intent,"evidence":"Whole selected evidence."});
    preparation::stage(&local, &original, &Default::default(), "JEV_CONTEXT_LIMIT").unwrap();
    let prompt = redelivery::prompt(&home, &store, &state, &request, decision).unwrap();
    assert!(prompt.contains("Prepare context for ONE unchanged Jev decision"));
    assert_eq!(
        fs::read_to_string(input.join("request.json")).unwrap(),
        "retained report packet"
    );
    let job: preparation::Job = kah_cli::observer_review::source_store::read(
        &store.directory.join(format!("preparation-{decision}.json")),
    )
    .unwrap();
    assert_eq!(job.state, original);
    let root = store.directory.parent().unwrap().to_owned();
    drop(store);
    fs::remove_dir_all(root).unwrap();
    fs::remove_dir_all(home).unwrap();
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
#[test]
fn actual_warm_idle_timeout_retains_the_review_without_hiding_other_herdr_errors() {
    use kah_cli::observer_review::delivery::idle_wait;
    let real = "RUN_HERDR_COMMAND stage=pane: agent wait: {\"error\":{\"code\":\"timeout\",\"message\":\"timed out waiting for agent status\"},\"id\":\"cli:agent:wait\"}\n";
    assert!(
        idle_wait(Err(real.into()))
            .unwrap_err()
            .to_string()
            .starts_with("REVIEW_SUBMISSION_UNCONFIRMED stage=delivery:")
    );
    let other =
        "RUN_HERDR_COMMAND stage=pane: agent wait: {\"error\":{\"code\":\"agent_not_found\"}}";
    assert_eq!(idle_wait(Err(other.into())).unwrap_err().to_string(), other);
}

#[test]
fn an_undeliverable_reviewer_notice_is_durable_and_named() {
    use kah_cli::observer_review::redelivery;
    let worker = format!("reviewer-notice-unit-{}", std::process::id());
    let home = std::env::temp_dir().join(&worker);
    let state = setup(&worker, &home);
    let store = Store::open(&worker).unwrap();
    redelivery::begin(
        &store,
        "review-stalled",
        "REVIEW_REPORT_MISSING stage=observer",
    )
    .unwrap();
    redelivery::begin(
        &store,
        "review-stalled",
        "REVIEW_REPORT_MISSING stage=observer",
    )
    .unwrap();
    let root = store.directory.clone();
    drop(store);
    redelivery::notify(&state).unwrap();
    let notice: serde_json::Value =
        kah_cli::observer_review::state::read(&root.join("redelivery-reviewer-notice.json"))
            .unwrap()
            .unwrap();
    assert_eq!(notice["submitted"], false);
    assert_eq!(notice["notice"]["request"], "review-stalled");
    assert!(
        notice["notice"]["text"]
            .as_str()
            .unwrap()
            .contains("REVIEW_REPORT_MISSING")
    );
    fs::remove_dir_all(root.parent().unwrap()).unwrap();
}

#[test]
fn accepted_native_receipt_confirms_both_not_idle_and_agent_wait_timeouts() {
    let error = "RUN_HERDR_COMMAND stage=pane: agent wait: {\"error\":{\"code\":\"timeout\",\"message\":\"timed out waiting for agent status\"},\"id\":\"cli:agent:wait\"}";
    for _ in 0..2 {
        assert!(!kah_cli::observer_review::delivery::readiness(Err(error.into()), true).unwrap());
    }
    assert!(
        kah_cli::observer_review::delivery::readiness(Err(error.into()), false)
            .unwrap_err()
            .to_string()
            .starts_with("REVIEW_SUBMISSION_UNCONFIRMED")
    );
}

#[test]
fn late_native_confirmation_clears_failure_notice_before_reviewer_delivery() {
    use kah_cli::observer_review::{redelivery, state::Store};
    let worker = format!("late-confirmation-{}", std::process::id());
    let store = Store::open(&worker).unwrap();
    for _ in 0..2 {
        redelivery::begin(
            &store,
            "review-accepted",
            "REVIEW_SUBMISSION_UNCONFIRMED stage=delivery: observer not idle yet",
        )
        .unwrap();
    }
    assert!(store.directory.join("redelivery-notice.json").exists());
    redelivery::completed(&store, "review-accepted").unwrap();
    assert!(redelivery::take(&store).unwrap().is_none());
    let root = store.directory.clone();
    drop(store);
    std::fs::remove_dir_all(root.parent().unwrap()).unwrap();
}
