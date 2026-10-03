use kah_cli::{
    observer_review::{
        checkpoint,
        state::{Pending, Purpose, Request, State, Store},
    },
    relay_store::Endpoint,
};
use serde_json::json;
#[test]
fn checkpoint_receipts_cover_only_the_reviewed_commit_and_intent_and_preserve_failed_nudges() {
    let worker = format!("checkpoint-unit-{}", std::process::id());
    let home = std::env::temp_dir().join(&worker);
    let mut state = State::new(
        Endpoint {
            provider: "codex".into(),
            session: worker.clone(),
            project: home.clone(),
            pane: None,
            socket: None,
        },
        &home,
    );
    let store = Store::open(&worker).unwrap();
    let request = Request {
        id: "review-unit".into(),
        worker: worker.clone(),
        purpose: Purpose::Checkpoint,
        turn: "t".into(),
        head: Some("abc".into()),
        intent: json!({"revision":2}),
        trigger: json!({}),
        documents: vec![],
        created_ms: 0,
    };
    let result = json!({"assessment":{"passed":false,"unproven":["integration"],"deferrals":[{"id":"d","evidence":"requires phase 3"}]}});
    checkpoint::record(&store, &mut state, &request, &result).unwrap();
    assert_eq!(checkpoint::covers(&state, "abc", 2), Some(false));
    assert_eq!(checkpoint::covers(&state, "def", 2), None);
    assert_eq!(checkpoint::covers(&state, "abc", 3), None);
    assert_eq!(state.deferrals.len(), 1);
    let reminder: serde_json::Value =
        serde_json::from_slice(&std::fs::read(store.directory.join("reminder.json")).unwrap())
            .unwrap();
    assert_eq!(reminder["purpose"], "checkpoint");
    state.checkpoint.as_mut().unwrap()["passed"] = true.into();
    assert_eq!(checkpoint::covers(&state, "abc", 2), Some(true));
    state.pending = Some(Pending::from(&request));
    assert_eq!(checkpoint::covers(&state, "abc", 2), None);
    let directory = store.directory.clone();
    drop(store);
    std::fs::remove_dir_all(directory.parent().unwrap()).unwrap();
}
