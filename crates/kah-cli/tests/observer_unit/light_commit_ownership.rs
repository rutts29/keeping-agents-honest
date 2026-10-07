//! USER light reviews must never close or overwrite the heavy gate verdict.
use crate::observer_review::{
    consume,
    state::{Pending, Purpose, Request, State, Store},
};
use serde_json::json;
#[test]
fn light_result_releases_its_lease_but_keeps_heavy_checkpoint_identity() {
    let worker = format!("light-owner-{}", crate::relay_store::nonce());
    let store = Store::open(&worker).unwrap();
    let request = Request {
        id: "review-light-owner".into(),
        worker: worker.clone(),
        purpose: Purpose::Checkpoint,
        turn: "t".into(),
        head: Some("a".repeat(40)),
        intent: json!({"revision":1}),
        trigger: json!({"review_class":"commit"}),
        documents: vec![],
        created_ms: 0,
    };
    let original = json!({"head":"a".repeat(40),"request":"review-heavy","passed":null,"state":"checkpoint_needs_thinker","intent_revision":1});
    let mut state:State=serde_json::from_value(json!({"worker":{"provider":"codex","session":worker,"project":"/tmp/fixture","pane":null,"socket":null},"observer":{"session":null,"pane":null,"name":"fixture","workspace":"/tmp/fixture","model":"gpt-6.1-sol","effort":"low","transcript":null},"pending":null,"latest":null,"last_verified_turn":null,"checkpoint":original,"deferrals":[],"wakes":0,"native_tokens":0,"stopped":false})).unwrap();
    state.pending = Some(Pending::from(&request));
    consume::commit_result(
        &store,
        &mut state,
        &request,
        json!({"assessment":{"passed":true,"state":"commit_passed"}}),
        None,
    )
    .unwrap();
    let saved = store.load().unwrap().unwrap();
    assert!(saved.pending.is_none());
    assert_eq!(saved.checkpoint, Some(original));
    assert_eq!(
        saved.latest.unwrap()["assessment"]["state"],
        "commit_passed"
    );
    let root = store.directory.parent().unwrap().to_owned();
    drop(store);
    std::fs::remove_dir_all(root).unwrap();
}
