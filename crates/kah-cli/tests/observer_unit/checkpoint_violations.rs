use super::*;
use crate::observer_review::{
    checkpoint,
    state::{Purpose, Request, State},
};

fn state(root: &Path) -> State {
    serde_json::from_value(json!({
        "worker":{"provider":"codex","session":"worker","project":root,"pane":null,"socket":null},
        "observer":{"session":null,"pane":null,"name":"fixture","workspace":root,"model":"gpt-6.1-sol","effort":"low","transcript":null},
        "pending":null,"latest":null,"last_verified_turn":null,"checkpoint":null,"deferrals":[],"wakes":0,"native_tokens":0,"stopped":false
    })).unwrap()
}
fn request() -> Request {
    Request {
        id: "review-checkpoint".into(),
        worker: "worker".into(),
        purpose: Purpose::Checkpoint,
        turn: "t".into(),
        head: Some("a".repeat(40)),
        intent: json!({"revision":1}),
        trigger: json!({}),
        documents: vec![],
        created_ms: 0,
    }
}

#[test]
fn later_clean_drift_cannot_clear_checkpoint_owned_confirmed_violations() {
    let root =
        std::env::temp_dir().join(format!("kah-checkpoint-violations-{}", std::process::id()));
    let mut store = Store::delivery_at(&root, false).unwrap();
    // The production owner holds the state lock, not the reminder-delivery lock.
    store.directory = root.join("review-state");
    kah_core::run_store::ensure_private_directory(&store.directory).unwrap();
    let mut state = state(&root);
    let result = json!({"assessment":{"state":"checkpoint_failed","passed":false,"unproven":["integration"],"violations":["integration"]}});
    checkpoint::record(&store, &mut state, &request(), &result).unwrap();
    state.latest = Some(json!({"purpose":"drift","assessment":{"passed":true,"violations":[]}}));
    assert_eq!(confirmed_violations(&state), vec!["integration"]);
    assert_eq!(
        state.checkpoint.as_ref().unwrap()["violations"],
        json!(["integration"])
    );
    state.latest = Some(json!({"assessment":{"violations":["intent","integration"]}}));
    assert_eq!(confirmed_violations(&state), vec!["integration", "intent"]);
    state.latest = None;
    assert_eq!(confirmed_violations(&state), vec!["integration"]);
    let mut clear = result;
    clear["assessment"]["violations"] = json!([]);
    checkpoint::record(&store, &mut state, &request(), &clear).unwrap();
    assert!(confirmed_violations(&state).is_empty());
    drop(store);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn absent_checkpoint_and_no_violation_reviews_keep_approval_unblocked() {
    let mut state = state(Path::new("/disposable"));
    assert!(confirmed_violations(&state).is_empty());
    state.checkpoint = Some(json!({"head":"old","intent_revision":0}));
    state.latest = Some(json!({"assessment":{"violations":[]}}));
    assert!(confirmed_violations(&state).is_empty());
}

#[test]
fn approval_cannot_close_another_review_on_the_same_sha_and_revision() {
    let mut state = state(Path::new("/disposable"));
    state.checkpoint =
        Some(json!({"request":"review-new","head":"a".repeat(40),"intent_revision":1}));
    let old =
        json!({"worker":"worker","request":"review-old","head":"a".repeat(40),"intent_revision":1});
    assert!(same_checkpoint(&state, &old).is_err());
    let mut current = old;
    current["request"] = "review-new".into();
    assert!(same_checkpoint(&state, &current).is_ok());
    current["worker"] = "other-worker".into();
    assert!(same_checkpoint(&state, &current).is_err());
}

#[test]
fn same_scope_recheck_preserves_the_pending_questions_checkpoint_identity() {
    let root = std::env::temp_dir().join(format!(
        "kah-pending-checkpoint-owner-{}",
        std::process::id()
    ));
    let mut store = Store::delivery_at(&root, false).unwrap();
    store.directory = root.join("state");
    for path in [
        store.directory.clone(),
        store.directory.join("sources"),
        store.directory.join("questions"),
    ] {
        kah_core::run_store::ensure_private_directory(&path).unwrap();
    }
    crate::observer_review::source_store::save(&store.directory.join("questions"),"question-one.json",&json!({"kind":"checkpoint_approval","status":"pending","worker":"worker","request":"review-original","head":"a".repeat(40),"intent_revision":1,"id":"question-one","delivery":"submitted"})).unwrap();
    let mut state = state(&root);
    let mut later = request();
    later.id = "review-later-claim".into();
    let result = json!({"assessment":{"state":"checkpoint_needs_thinker","passed":null,"needs_thinker":["integration"],"needs_user":[],"unproven":["integration"],"violations":[]}});
    checkpoint::record(&store, &mut state, &later, &result).unwrap();
    let question = json!({"worker":"worker","request":"review-original","head":"a".repeat(40),"intent_revision":1});
    assert!(same_checkpoint(&state, &question).is_ok());
    assert_eq!(
        state.checkpoint.as_ref().unwrap()["request"],
        "review-original"
    );
    drop(store);
    fs::remove_dir_all(root).unwrap();
}
