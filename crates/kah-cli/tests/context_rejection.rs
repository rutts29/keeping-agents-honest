use kah_cli::observer_review::{
    preparation::{self, Job},
    state::{Purpose, Request, Store},
};
use kah_observer::context_selection::Manifest;
use serde_json::json;
#[test]
fn same_rejected_whole_context_is_terminal_but_changed_context_gets_a_revision() {
    let worker = format!("context-rejection-{}", kah_cli::relay_store::nonce());
    let store = Store::open(&worker).unwrap();
    let request = Request {
        id: "review-original-v8-one".into(),
        worker: worker.clone(),
        purpose: Purpose::Drift,
        turn: "turn".into(),
        head: None,
        intent: json!({"goal":"Whole verbatim goal"}),
        trigger: json!({}),
        documents: vec![],
        created_ms: 1,
    };
    let job = Job {
        worker,
        decision: request.id.clone(),
        state: json!({"intent":request.intent,"whole":"Whole uncut context."}),
        questions: Default::default(),
        items: vec![],
        reason: "provider rejected".into(),
    };
    let manifest = Manifest {
        decision: request.id.clone(),
        selected: vec![],
        excluded: vec![],
    };
    let prepared = json!({"intent":request.intent,"whole":"Whole uncut context."});
    assert!(
        !preparation::rejection::record(&request, &job, &manifest, &prepared, "JEV_CONTEXT_LIMIT")
            .unwrap()
    );
    assert!(
        preparation::rejection::record(&request, &job, &manifest, &prepared, "JEV_CONTEXT_LIMIT")
            .unwrap()
    );
    assert!(
        !preparation::rejection::record(
            &request,
            &job,
            &manifest,
            &json!({"intent":request.intent,"whole":"Different whole relevant context."}),
            "JEV_CONTEXT_LIMIT"
        )
        .unwrap()
    );
    let root = store.directory.parent().unwrap().to_owned();
    drop(store);
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn terminal_context_releases_the_slot_without_passing_or_erasing_a_confirmed_violation() {
    use kah_cli::observer_review::state::{Pending, State};
    let worker = format!("context-observation-{}", kah_cli::relay_store::nonce());
    let store = Store::open(&worker).unwrap();
    let request = Request {
        id: "review-terminal".into(),
        worker: worker.clone(),
        purpose: Purpose::Drift,
        turn: "turn".into(),
        head: Some("a".repeat(40)),
        intent: json!({"goal":"Whole goal"}),
        trigger: json!({}),
        documents: vec![],
        created_ms: 1,
    };
    let mut state:State=serde_json::from_value(json!({"worker":{"provider":"codex","session":worker,"project":store.directory,"pane":null,"socket":null},"observer":{"session":"same-observer","pane":"same-pane","name":"fixture","workspace":store.directory,"model":"gpt-6.1-sol","effort":"low","transcript":null},"pending":Pending::from(&request),"latest":null,"last_verified_turn":null,"checkpoint":null,"deferrals":[],"wakes":0,"native_tokens":0,"stopped":false})).unwrap();
    let result = json!({"request":request.id,"purpose":"drift","head":request.head,"jev_state":{"unconfirmed_context":"PREPARATION_CONTEXT_UNCONFIRMED"},"assessment":{"passed":false,"violations":["confirmed-user-prohibition"]},"judgment":{"backend":"jev","answers":{"unchanged-real-answer":true}}});
    preparation::rejection::park(&store, &mut state, &request, &result).unwrap();
    assert!(state.pending.is_none());
    assert_eq!(state.observer.session.as_deref(), Some("same-observer"));
    assert_eq!(
        state.latest.as_ref().unwrap()["assessment"]["passed"],
        false
    );
    assert_eq!(
        state.latest.as_ref().unwrap()["assessment"]["violations"],
        result["assessment"]["violations"]
    );
    assert!(store.directory.join("parked/review-terminal.json").exists());
    let root = store.directory.parent().unwrap().to_owned();
    drop(store);
    std::fs::remove_dir_all(root).unwrap();
}
