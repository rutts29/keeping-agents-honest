use super::*;
use serde_json::json;
use std::io::Cursor;

const SESSION: &str = "00000000-0000-4000-8000-000000000001";
const REQUEST: &str = "review-1000000-1";
fn fixture_path(root: &Path, value: &serde_json::Value) -> std::path::PathBuf {
    let relative = Path::new(value.as_str().unwrap());
    assert!(!relative.as_os_str().is_empty());
    assert!(
        relative
            .components()
            .all(|part| matches!(part, std::path::Component::Normal(_)))
    );
    root.join(relative)
}
fn transcript() -> String {
    let meta = json!({"type":"session_meta","payload":{"id":SESSION,"session_id":SESSION,
        "cwd":"/observer","source":"cli","originator":"codex-tui","model_provider":"openai"}});
    let user = json!({"type":"response_item","payload":{"type":"message","role":"user","content":[{"type":"input_text","text":format!("{}kah review request {REQUEST} (Checkpoint). Read evidence.",crate::observer_review::delivery_text::INSTRUCTION)}]}});
    format!("{meta}\n{user}\n")
}
#[test]
fn native_receipt_recovers_without_any_hook_callback() {
    let text = transcript();
    let proof = verify(
        Cursor::new(&text),
        SESSION,
        Path::new("/observer"),
        &format!("kah review request {REQUEST} "),
    )
    .unwrap()
    .expect("native receipt owns binding even without hook callback");
    assert_eq!(proof.session, SESSION);
    assert!(proof.receipt_offset > 0);
    assert_eq!(proof.receipt_end, text.len() as u64);
}
#[test]
fn wrong_session_workspace_provider_or_request_never_confers_a_binding() {
    let text = transcript();
    let marker = format!("kah review request {REQUEST} ");
    for bad in [
        text.replace(SESSION, "different-session"),
        text.replace("/observer", "/different"),
        text.replace("openai", "anthropic"),
    ] {
        assert!(verify(Cursor::new(bad), SESSION, Path::new("/observer"), &marker).is_err());
    }
    assert!(
        verify(
            Cursor::new(text),
            SESSION,
            Path::new("/observer"),
            "kah review request review-other "
        )
        .unwrap()
        .is_none()
    );
}
#[test]
fn an_assistant_echo_or_partial_request_id_is_not_a_native_receipt() {
    let marker = format!("kah review request {REQUEST} ");
    for text in [
        transcript().replace("\"role\":\"user\"", "\"role\":\"assistant\""),
        transcript().replace(REQUEST, &format!("{REQUEST}-other")),
    ] {
        assert!(
            verify(Cursor::new(text), SESSION, Path::new("/observer"), &marker)
                .unwrap()
                .is_none()
        );
    }
}
#[test]
#[ignore = "private retained native corpus; no model or provider call"]
fn retained_startup4_native_record_proves_the_original_unbound_submission() {
    let root = std::path::PathBuf::from(std::env::var_os("KAH_STARTUP4_CORPUS").unwrap());
    let case: serde_json::Value =
        serde_json::from_slice(&std::fs::read(root.join("fixture.json")).unwrap()).unwrap();
    let session = case["session"].as_str().unwrap();
    let request = case["request"].as_str().unwrap();
    let file = fixture_path(&root, &case["transcript"]);
    let cwd = fixture_path(&root, &case["workspace"]);
    assert!(root.join("handoff-diagnosis.json").exists());
    let proof = verify(
        std::io::BufReader::new(std::fs::File::open(&file).unwrap()),
        session,
        &cwd,
        &format!("kah review request {request} "),
    )
    .unwrap()
    .expect("retained real receipt must bind");
    assert_eq!(proof.session, session);
    let (store, mut state, _) = binding_fixture();
    state.observer.workspace = cwd;
    state.pending.as_mut().unwrap().id = request.into();
    store.save(&state).unwrap();
    let worker = state.worker.session.clone();
    let submitted = state
        .observer
        .workspace
        .join("outbox")
        .join(format!("submitted-{request}.json"));
    let before = std::fs::read(&submitted).unwrap();
    drop(store);
    assert!(recovery::persist(&state, &file, &proof).unwrap());
    let store = crate::observer_review::state::Store::open(&worker).unwrap();
    assert!(
        crate::observer_review::liveness::handoff::awaiting_host(&store.load().unwrap().unwrap())
            .unwrap()
    );
    assert_eq!(std::fs::read(&submitted).unwrap(), before);
    let private_root = store.directory.parent().unwrap().to_owned();
    drop(store);
    std::fs::remove_dir_all(private_root).unwrap();
}

fn binding_fixture() -> (
    crate::observer_review::state::Store,
    crate::observer_review::state::State,
    std::path::PathBuf,
) {
    use crate::observer_review::state::{Pending, Purpose, State, Store};
    let worker = format!("native-binding-{}", crate::relay_store::nonce());
    let store = Store::open(&worker).unwrap();
    let workspace = store.directory.join("observer");
    let mut state: State = serde_json::from_value(json!({
        "worker":{"provider":"codex","session":worker,"project":store.directory,"pane":null,"socket":null},
        "observer":{"session":null,"pane":null,"name":"fixture","workspace":workspace,"model":"gpt-6.1-sol","effort":"low","transcript":null},
        "pending":null,"latest":null,"last_verified_turn":null,"checkpoint":null,"deferrals":[],
        "wakes":0,"native_tokens":0,"stopped":false
    })).unwrap();
    state.observer.pane = Some("w0:p0".into());
    state.pending = Some(Pending {
        id: REQUEST.into(),
        purpose: Purpose::Checkpoint,
        turn: "fixture-turn".into(),
        head: None,
        created_ms: 0,
        native_tokens_before: None,
    });
    std::fs::create_dir_all(state.observer.workspace.join("outbox")).unwrap();
    store.save(&state).unwrap();
    let path = store.directory.join("native.jsonl");
    (store, state, path)
}
#[test]
fn a_submitted_report_without_hooks_is_acknowledged_once_without_changing_its_bytes() {
    let (store, state, path) = binding_fixture();
    let submitted = state
        .observer
        .workspace
        .join("outbox")
        .join(format!("submitted-{REQUEST}.json"));
    let bytes = serde_json::to_vec(&json!({"request":REQUEST})).unwrap();
    std::fs::write(&submitted, &bytes).unwrap();
    let text = transcript().replace("/observer", state.observer.workspace.to_str().unwrap());
    std::fs::write(&path, &text).unwrap();
    let proof = verify(
        Cursor::new(&text),
        SESSION,
        &state.observer.workspace,
        &format!("kah review request {REQUEST} "),
    )
    .unwrap()
    .unwrap();
    let worker = state.worker.session.clone();
    drop(store);
    assert!(recovery::persist(&state, &path, &proof).unwrap());
    let store = crate::observer_review::state::Store::open(&worker).unwrap();
    let saved = store.load().unwrap().unwrap();
    assert_eq!(saved.observer.session.as_deref(), Some(SESSION));
    assert_eq!(std::fs::read(&submitted).unwrap(), bytes);
    assert!(crate::observer_review::liveness::handoff::awaiting_host(&saved).unwrap());
    assert!(!store.directory.join("hook-events").exists());
    assert!(
        store
            .directory
            .join("native-binding-receipts")
            .join(format!("{REQUEST}.json"))
            .exists()
    );
    let root = store.directory.parent().unwrap().to_owned();
    drop(store);
    std::fs::remove_dir_all(root).unwrap();
}
#[test]
fn a_newer_pending_request_or_changed_pane_cannot_inherit_the_receipt() {
    let (store, state, path) = binding_fixture();
    let text = transcript().replace("/observer", state.observer.workspace.to_str().unwrap());
    let proof = verify(
        Cursor::new(text),
        SESSION,
        &state.observer.workspace,
        &format!("kah review request {REQUEST} "),
    )
    .unwrap()
    .unwrap();
    let mut changed = state.clone();
    changed.pending.as_mut().unwrap().id = "review-newer".into();
    store.save(&changed).unwrap();
    let worker = state.worker.session.clone();
    drop(store);
    assert!(recovery::persist(&state, &path, &proof).is_err());
    let store = crate::observer_review::state::Store::open(&worker).unwrap();
    changed = state.clone();
    changed.observer.pane = Some("other-pane".into());
    store.save(&changed).unwrap();
    drop(store);
    assert!(recovery::persist(&state, &path, &proof).is_err());
    let store = crate::observer_review::state::Store::open(&worker).unwrap();
    assert!(store.load().unwrap().unwrap().observer.session.is_none());
    let root = store.directory.parent().unwrap().to_owned();
    drop(store);
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn advisory_id_without_owned_pane_scope_is_rejected() {
    let (store, state, _) = binding_fixture();
    let pane = json!({"pane_id":"w0:p0","agent":"codex","foreground_cwd":state.observer.workspace,
        "agent_session":{"kind":"id","source":"herdr:codex","value":SESSION}});
    assert_eq!(recovery::pane_session(&state, &pane).unwrap(), SESSION);
    for (field, value) in [
        ("pane_id", "other"),
        ("agent", "claude"),
        ("foreground_cwd", "/different"),
    ] {
        let mut changed = pane.clone();
        changed[field] = json!(value);
        assert!(recovery::pane_session(&state, &changed).is_err());
    }
    let mut changed = pane;
    changed["agent_session"]["source"] = json!("pid-file");
    assert!(recovery::pane_session(&state, &changed).is_err());
    let root = store.directory.parent().unwrap().to_owned();
    drop(store);
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn submission_arriving_after_native_binding_is_acknowledged_without_a_callback() {
    let (store, state, path) = binding_fixture();
    let worker = state.worker.session.clone();
    let text = transcript().replace("/observer", state.observer.workspace.to_str().unwrap());
    std::fs::write(&path, &text).unwrap();
    let proof = verify(
        Cursor::new(text),
        SESSION,
        &state.observer.workspace,
        &format!("kah review request {REQUEST} "),
    )
    .unwrap()
    .unwrap();
    drop(store);
    assert!(!recovery::persist(&state, &path, &proof).unwrap());
    let store = crate::observer_review::state::Store::open(&worker).unwrap();
    let bound = store.load().unwrap().unwrap();
    let submitted = bound
        .observer
        .workspace
        .join("outbox")
        .join(format!("submitted-{REQUEST}.json"));
    std::fs::write(
        &submitted,
        serde_json::to_vec(&json!({"request":REQUEST})).unwrap(),
    )
    .unwrap();
    drop(store);
    assert!(recovery::acknowledge_bound(&bound).unwrap());
    assert!(!recovery::acknowledge_bound(&bound).unwrap());
    let store = crate::observer_review::state::Store::open(&worker).unwrap();
    assert!(
        crate::observer_review::liveness::handoff::awaiting_host(&store.load().unwrap().unwrap())
            .unwrap()
    );
    let root = store.directory.parent().unwrap().to_owned();
    drop(store);
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn warm_submission_gets_its_own_receipt_after_the_first_request_was_consumed() {
    let (store, state, path) = binding_fixture();
    let worker = state.worker.session.clone();
    let first = transcript().replace("/observer", state.observer.workspace.to_str().unwrap());
    std::fs::write(&path, &first).unwrap();
    let proof = verify(
        Cursor::new(&first),
        SESSION,
        &state.observer.workspace,
        &format!("kah review request {REQUEST} "),
    )
    .unwrap()
    .unwrap();
    drop(store);
    recovery::persist(&state, &path, &proof).unwrap();
    let store = crate::observer_review::state::Store::open(&worker).unwrap();
    let mut bound = store.load().unwrap().unwrap();
    let next = "review-1000000-2";
    bound.pending.as_mut().unwrap().id = next.into();
    store.save(&bound).unwrap();
    let submitted = bound
        .observer
        .workspace
        .join("outbox")
        .join(format!("submitted-{next}.json"));
    let bytes = serde_json::to_vec(&json!({"request":next})).unwrap();
    std::fs::write(&submitted, &bytes).unwrap();
    let later = first.lines().nth(1).unwrap().replace(REQUEST, next);
    std::fs::write(&path, format!("{first}{later}\n")).unwrap();
    let root = store.directory.parent().unwrap().to_owned();
    drop(store);
    assert!(
        recovery::acknowledge_bound(&bound).unwrap(),
        "later submitted request must hand off"
    );
    assert!(!recovery::acknowledge_bound(&bound).unwrap());
    assert_eq!(std::fs::read(&submitted).unwrap(), bytes);
    assert!(crate::observer_review::liveness::handoff::awaiting_host(&bound).unwrap());
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn working_or_unbound_submitted_requests_are_never_visibly_redelivered() {
    let (store, mut state, _) = binding_fixture();
    assert!(crate::observer_review::delivery::retry_held(&state, true));
    assert!(!crate::observer_review::delivery::retry_held(&state, false));
    let submitted = state
        .observer
        .workspace
        .join("outbox")
        .join(format!("submitted-{REQUEST}.json"));
    std::fs::write(&submitted, b"{}").unwrap();
    assert!(crate::observer_review::delivery::retry_held(&state, false));
    state.observer.session = Some(SESSION.into());
    assert!(crate::observer_review::delivery::retry_held(&state, false));
    state.observer.transcript = Some(store.directory.join("native.jsonl"));
    assert!(!crate::observer_review::delivery::retry_held(&state, false));
    std::fs::remove_dir_all(store.directory.parent().unwrap()).unwrap();
}

#[test]
#[ignore = "private retained native corpus; no model or provider call"]
fn retained_startup5_warm_submission_hands_off_existing_bytes() {
    let root = std::path::PathBuf::from(std::env::var_os("KAH_STARTUP5_CORPUS").unwrap());
    let case: serde_json::Value =
        serde_json::from_slice(&std::fs::read(root.join("fixture.json")).unwrap()).unwrap();
    let session = case["session"].as_str().unwrap();
    let first = case["first_request"].as_str().unwrap();
    let next = case["next_request"].as_str().unwrap();
    let native = fixture_path(&root, &case["transcript"]);
    let text = std::fs::read_to_string(&native).unwrap();
    assert_eq!(successful_submissions(&text, next), 4);
    let (store, mut state, _) = binding_fixture();
    state.observer.workspace = fixture_path(&root, &case["workspace"]);
    state.pending.as_mut().unwrap().id = first.into();
    store.save(&state).unwrap();
    let first_proof = verify(
        Cursor::new(&text),
        session,
        &state.observer.workspace,
        &format!("kah review request {first} "),
    )
    .unwrap()
    .unwrap();
    let worker = state.worker.session.clone();
    drop(store);
    recovery::persist(&state, &native, &first_proof).unwrap();
    let store = crate::observer_review::state::Store::open(&worker).unwrap();
    let mut bound = store.load().unwrap().unwrap();
    bound.pending.as_mut().unwrap().id = next.into();
    store.save(&bound).unwrap();
    let submitted = bound
        .observer
        .workspace
        .join("outbox")
        .join(format!("submitted-{next}.json"));
    let before = std::fs::read(&submitted).unwrap();
    let private_root = store.directory.parent().unwrap().to_owned();
    drop(store);
    assert!(recovery::acknowledge_bound(&bound).unwrap());
    assert!(crate::observer_review::liveness::handoff::awaiting_host(&bound).unwrap());
    assert!(!recovery::acknowledge_bound(&bound).unwrap());
    assert_eq!(std::fs::read(&submitted).unwrap(), before);
    assert!(!private_root.join("review/hook-events").exists());
    std::fs::remove_dir_all(private_root).unwrap();
}

fn successful_submissions(text: &str, request: &str) -> usize {
    text.lines()
        .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
        .filter(|row| {
            row["type"] == "event_msg"
                && row["payload"]["type"] == "task_complete"
                && row["payload"]["last_agent_message"]
                    .as_str()
                    .is_some_and(|s| s.contains(request))
        })
        .count()
}

#[test]
fn a_later_report_without_its_exact_native_request_receipt_stays_unbound() {
    let (store, mut state, path) = binding_fixture();
    state.observer.session = Some(SESSION.into());
    state.observer.transcript = Some(path.clone());
    state.pending.as_mut().unwrap().id = "review-not-delivered".into();
    store.save(&state).unwrap();
    let text = transcript().replace("/observer", state.observer.workspace.to_str().unwrap());
    std::fs::write(&path, text).unwrap();
    let submitted = state
        .observer
        .workspace
        .join("outbox/submitted-review-not-delivered.json");
    std::fs::write(&submitted, b"{}").unwrap();
    let root = store.directory.parent().unwrap().to_owned();
    drop(store);
    let error = recovery::acknowledge_bound(&state).unwrap_err().to_string();
    assert!(error.starts_with("REVIEW_NATIVE_UNBOUND"), "{error}");
    assert!(std::fs::read(&submitted).is_ok());
    assert!(!root.join("review/report-ready.json").exists());
    std::fs::remove_dir_all(root).unwrap();
}

fn launched_views(state: &crate::observer_review::state::State) -> [serde_json::Value; 3] {
    let cwd = &state.observer.workspace;
    [
        json!({"pane_id":"w0:p0","agent":null,"foreground_cwd":cwd}),
        json!({"pane_id":"w0:p0","agent":"codex","foreground_cwd":cwd}),
        json!({"pane_id":"w0:p0","agent":"codex","foreground_cwd":cwd,
            "agent_session":{"kind":"id","source":"herdr:codex","value":SESSION}}),
    ]
}

#[test]
fn a_launching_observer_binds_through_its_shell_and_unpublished_session_phases() {
    let (store, state, _) = binding_fixture();
    let [shell, unpublished, published] = launched_views(&state);
    assert_eq!(
        recovery::starting(&state, &shell),
        Some("native agent not yet detected")
    );
    assert_eq!(
        recovery::starting(&state, &unpublished),
        Some("native session not yet published")
    );
    assert_eq!(recovery::starting(&state, &published), None);
    assert_eq!(recovery::pane_session(&state, &published).unwrap(), SESSION);
    // Each first-seen phase restarts the cold window; repeats and other errors do not.
    let mut seen = Vec::new();
    let phase = format!("{STARTING} stage=observer_binding: native agent not yet detected");
    assert_eq!(startup_progress(&mut seen, &phase), Some(true));
    assert_eq!(startup_progress(&mut seen, &phase), Some(false));
    let unbound =
        "REVIEW_NATIVE_UNBOUND stage=observer_binding: exact request has no native receipt yet";
    assert_eq!(startup_progress(&mut seen, &unbound), Some(true));
    assert_eq!(
        startup_progress(&mut seen, &"REVIEW_STATE_MISSING stage=delivery"),
        None
    );
    let root = store.directory.parent().unwrap().to_owned();
    drop(store);
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn a_different_agent_cwd_process_or_bound_session_is_never_starting() {
    let (store, mut state, _) = binding_fixture();
    let [shell, unpublished, published] = launched_views(&state);
    let mut other = unpublished.clone();
    other["agent"] = json!("claude");
    assert_eq!(recovery::starting(&state, &other), None);
    let field = crate::observer_review::control::pane_mismatch(&state, &other).unwrap();
    assert_eq!(
        (field["field"].clone(), field["observed"].clone()),
        (json!("agent"), json!("claude"))
    );
    for view in [&shell, &unpublished] {
        let mut outside = view.clone();
        outside["foreground_cwd"] = json!("/");
        assert_eq!(recovery::starting(&state, &outside), None);
    }
    let mut wrong = published.clone();
    wrong["agent_session"]["source"] = json!("pid-file");
    assert_eq!(recovery::starting(&state, &wrong), None);
    let error = recovery::pane_session(&state, &wrong)
        .unwrap_err()
        .to_string();
    assert!(
        error.ends_with("pane identity is not the owned native observer: agent_session"),
        "{error}"
    );
    let saved: serde_json::Value = serde_json::from_slice(
        &std::fs::read(store.directory.join("identity-mismatch.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(saved["observed"]["source"], "pid-file");
    assert_eq!(saved["check"], "pane_session");
    // A recorded process that no longer matches its stamp is a replacement.
    state.observer.pid = Some(std::process::id());
    state.observer.process_stamp = kah_observer::session::process_stamp(std::process::id());
    assert!(crate::observer_review::control::pane_mismatch(&state, &unpublished).is_none());
    state.observer.process_stamp = Some("replaced".into());
    assert_eq!(recovery::starting(&state, &shell), None);
    assert_eq!(
        crate::observer_review::control::pane_mismatch(&state, &unpublished).unwrap()["field"],
        "process_stamp"
    );
    // Once a session is bound, startup phases are mismatches again.
    state.observer.pid = None;
    state.observer.session = Some(SESSION.into());
    assert_eq!(recovery::starting(&state, &shell), None);
    assert_eq!(recovery::starting(&state, &unpublished), None);
    let root = store.directory.parent().unwrap().to_owned();
    drop(store);
    std::fs::remove_dir_all(root).unwrap();
}
