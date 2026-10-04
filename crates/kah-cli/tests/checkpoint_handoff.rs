use kah_cli::observer_review::{
    events, queue,
    state::{State, Store},
};
use serde_json::json;
#[test]
fn busy_checkpoint_transfer_is_queued_once_and_cannot_repeat_itself_after_review() {
    let worker = format!("checkpoint-handoff-unit-{}", std::process::id());
    let store = Store::open(&worker).unwrap();
    let state:State=serde_json::from_value(json!({"worker":{"provider":"codex","session":worker,"project":"/tmp","pane":null,"socket":null},"observer":{"session":null,"pane":null,"name":"fixture","workspace":"/tmp","model":"gpt-6.1-sol","effort":"low","transcript":null},"pending":null,"latest":null,"last_verified_turn":null,"checkpoint":null,"deferrals":[],"wakes":0,"native_tokens":0,"stopped":false})).unwrap();
    store.save(&state).unwrap();
    let root = store.directory.join("checkpoint-events");
    kah_core::run_store::ensure_private_directory(&root).unwrap();
    let whole = json!({"kind":"user_gate","at_ms":1,"meaning":"whole original event"});
    kah_cli::unattended_report::save(&root, "first.json", &whole).unwrap();
    events::run(&worker).unwrap();
    assert!(
        !events::pending(&worker).unwrap(),
        "the event was already durable in the review queue"
    );
    events::run(&worker).unwrap();
    let jobs = queue::entries(&worker).unwrap();
    assert_eq!(jobs.len(), 1);
    assert_eq!(jobs[0].1.trigger["events"][0], whole);
    let directory = store.directory.parent().unwrap().to_path_buf();
    drop(store);
    std::fs::remove_dir_all(directory).unwrap();
}
