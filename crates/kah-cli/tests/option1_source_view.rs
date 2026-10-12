//! Completed Option1 reports must release disposable views, keeping private CAS.
use kah_cli::observer_review::{
    option1, source_store,
    state::{Pending, Purpose, Request, State, Store},
};
use serde_json::json;
use std::{fs, path::Path};

#[test]
fn ordinary_option1_consume_releases_its_view_and_preserves_private_evidence() {
    // All product Store calls below inherit the checked private XDG root.
    std::env::var_os("XDG_STATE_HOME").expect("run through demos/check-tests.sh");
    let worker = format!("source-view-test-{}", std::process::id());
    let store = Store::open(&worker).unwrap();
    let root = std::env::temp_dir().join(&worker);
    fs::create_dir_all(root.join("outbox")).unwrap();
    source_store::initialize(&store.directory, &root).unwrap();
    let request = request(&worker);
    let state = state(&worker, &root, &request);
    store.save(&state).unwrap();
    let input = root.join("inputs").join(&request.id);
    fs::create_dir_all(input.join("sources")).unwrap();
    source_store::text(
        &store.directory,
        &input.join("record.txt"),
        "whole retained evidence",
    )
    .unwrap();
    fs::write(
        input.join("sources/disposable-copy"),
        "whole retained evidence",
    )
    .unwrap();
    fs::write(
        root.join("outbox")
            .join(format!("submitted-{}.json", request.id)),
        serde_json::to_vec(
            &json!({"request":request.id,"result":{"stage":"checkpoint","rows":[]}}),
        )
        .unwrap(),
    )
    .unwrap();
    for name in ["gap.json", "delivered-gap.json"] {
        kah_cli::unattended_report::save(&store.directory, name, &json!({"request":request.id,"error":"REVIEW_IO stage=review: No such file or directory (os error 2)"})).unwrap();
    }
    let private = store.directory.clone();
    option1::consume::run(store, state, request).unwrap();
    assert!(
        !private.join("gap.json").exists(),
        "successful consume clears only its own ghost gap"
    );
    assert!(!private.join("delivered-gap.json").exists());
    assert!(
        !input.join("sources").exists(),
        "completed report must release its view"
    );
    assert_eq!(
        source_store::load_text(&input.join("record.txt")).unwrap(),
        "whole retained evidence"
    );
    assert!(
        Store::open(&worker)
            .unwrap()
            .load()
            .unwrap()
            .unwrap()
            .pending
            .is_none()
    );
    fs::remove_dir_all(root).unwrap();
    fs::remove_dir_all(private.parent().unwrap()).unwrap();
}

fn request(worker: &str) -> Request {
    Request {
        id: "review-1".into(),
        worker: worker.into(),
        purpose: Purpose::Checkpoint,
        turn: "turn".into(),
        head: None,
        intent: json!({"revision":1}),
        documents: vec![],
        created_ms: 1,
        trigger: json!({"option1":{"stage":"checkpoint","original_trigger":{"kind":"native_git_tag"},
            "input":{"revision":"1","baseline":null,"checkpoint":true,"code_changed":false,
                "intent":{"goal":"retain evidence","items":{},"retired":[],"permissions":{},"conflicts":[]},
                "impacted":[],"rules":{},"hunks":{},"files":{},"matrix":[],"carried":{}}}}),
    }
}
fn state(worker: &str, root: &Path, request: &Request) -> State {
    let mut state: State = serde_json::from_value(json!({
        "worker":{"provider":"codex","session":worker,"project":root,"pane":null,"socket":null},
        "observer":{"session":"test-observer","pane":null,"name":"test-observer","workspace":root,
            "model":"gpt-6.1-sol","effort":"low","transcript":null},
        "pending":null,"latest":null,"last_verified_turn":null,"checkpoint":null,
        "deferrals":[],"wakes":0,"native_tokens":0,"stopped":false,
    }))
    .unwrap();
    state.pending = Some(Pending::from(request));
    state
}

#[test]
fn every_pending_transition_releases_only_the_previous_view() {
    std::env::var_os("XDG_STATE_HOME").expect("run through demos/check-tests.sh");
    let worker = format!("pending-view-test-{}", std::process::id());
    let store = Store::open(&worker).unwrap();
    let root = std::env::temp_dir().join(&worker);
    fs::create_dir_all(&root).unwrap();
    source_store::initialize(&store.directory, &root).unwrap();
    let mut old = request(&worker);
    old.purpose = Purpose::Preparation;
    let mut state = state(&worker, &root, &old);
    store.save(&state).unwrap();
    let old_view = root.join("inputs/review-1/sources");
    let new_view = root.join("inputs/review-2/sources");
    fs::create_dir_all(&old_view).unwrap();
    fs::create_dir_all(&new_view).unwrap();
    store.save(&state).unwrap();
    assert!(
        old_view.exists(),
        "active request stays readable on unrelated state writes"
    );
    let mut next = old.clone();
    next.id = "review-2".into();
    state.pending = Some(Pending::from(&next));
    store.save(&state).unwrap();
    assert!(
        !old_view.exists(),
        "superseded slot releases its previous view"
    );
    assert!(new_view.exists(), "successor view must survive the handoff");
    state.pending = None;
    store.save(&state).unwrap();
    assert!(
        !new_view.exists(),
        "preparation completion also releases the view"
    );
    let private = store.directory.clone();
    drop(store);
    fs::remove_dir_all(root).unwrap();
    fs::remove_dir_all(private.parent().unwrap()).unwrap();
}
