use super::*;
use serde_json::json;
use std::io::Cursor;

const SESSION: &str = "00000000-0000-4000-8000-100000000006";
const REQUEST: &str = "review-1000000000014-1";
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
    let file = root.join(format!(
        "home/.codex/sessions/2000/01/01/rollout-2000-01-01T00-00-00-{SESSION}.jsonl"
    ));
    let cwd = root.join("home/.cache/kah-observers/000000000000000000000002");
    assert!(root.join("handoff-diagnosis.json").exists());
    let proof = verify(
        std::io::BufReader::new(std::fs::File::open(&file).unwrap()),
        SESSION,
        &cwd,
        &format!("kah review request {REQUEST} "),
    )
    .unwrap()
    .expect("retained real receipt must bind");
    assert_eq!(proof.session, SESSION);
    let (store, mut state, _) = binding_fixture();
    state.observer.workspace = cwd;
    store.save(&state).unwrap();
    let worker = state.worker.session.clone();
    let submitted = state
        .observer
        .workspace
        .join("outbox")
        .join(format!("submitted-{REQUEST}.json"));
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
    state.observer.pane = Some("w999:p1".into());
    state.pending = Some(Pending {
        id: REQUEST.into(),
        purpose: Purpose::Checkpoint,
        turn: "retained-startup4".into(),
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
    let pane = json!({"pane_id":"w999:p1","agent":"codex","foreground_cwd":state.observer.workspace,
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
