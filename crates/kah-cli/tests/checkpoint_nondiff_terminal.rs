//! Non-diff unclear rows become a terminal finding, never another checkpoint.
#[allow(dead_code)]
mod checkpoint_replay;
use kah_cli::observer_review::{
    option1, source_store,
    state::{Pending, Purpose, Request, State, Store},
};
use serde_json::json;
use std::fs;

#[test]
fn nondiff_rows_finish_as_one_terminal_finding() {
    std::env::var_os("XDG_STATE_HOME").expect("run through demos/check-tests.sh");
    let worker = format!("empty-carry-{}", kah_cli::relay_store::nonce());
    let root = std::env::temp_dir().join(&worker);
    fs::create_dir_all(root.join("outbox")).unwrap();
    let store = Store::open(&worker).unwrap();
    source_store::initialize(&store.directory, &root).unwrap();
    let directory = store.directory.clone();
    let tag = &checkpoint_replay::tag();
    let end = tag;
    let request = Request {
        id: end["id"].as_str().unwrap().into(),
        worker: worker.clone(),
        purpose: Purpose::Checkpoint,
        turn: "recorded-end".into(),
        head: Some(checkpoint_replay::HEAD.into()),
        intent: json!({"revision":2}),
        documents: vec![],
        created_ms: 1,
        trigger: json!({"option1":{"stage":"checkpoint","input":end["input"],"original_trigger":end["trigger"]}}),
    };
    let mut state:State=serde_json::from_value(json!({"worker":{"provider":"claude","session":worker,"project":root,"pane":null,"socket":null},
        "observer":{"session":"recorded-observer","pane":null,"name":"recorded-observer","workspace":root,"model":"claude-sonnet-5-5","effort":"low","transcript":null},
        "pending":null,"latest":tag["latest"],"last_verified_turn":null,"checkpoint":null,"deferrals":[],"wakes":0,"native_tokens":0,"stopped":false})).unwrap();
    state.pending = Some(Pending::from(&request));
    store.save(&state).unwrap();
    fs::write(
        root.join("outbox")
            .join(format!("submitted-{}.json", request.id)),
        serde_json::to_vec(&end["report"]).unwrap(),
    )
    .unwrap();
    kah_cli::unattended_report::save(
        &store.directory,
        "report-ready.json",
        &json!({"request":request.id}),
    )
    .unwrap();
    option1::consume::run(store, state, request).unwrap();
    let saved = Store::read(&worker).unwrap().unwrap();
    assert!(saved.pending.is_none());
    verify_terminal(&saved, &directory, checkpoint_replay::HEAD);
    fs::remove_dir_all(directory.parent().unwrap()).unwrap();
    fs::remove_dir_all(root).unwrap();
}

fn verify_terminal(saved: &State, directory: &std::path::Path, head: &str) {
    let checkpoint = saved.checkpoint.as_ref().unwrap();
    assert!(checkpoint["passed"].is_null());
    assert_eq!(checkpoint["terminal"], true);
    assert_eq!(
        checkpoint["unverified_non_diff"].as_array().unwrap().len(),
        9
    );
    assert!(
        checkpoint["unverified_non_diff"]
            .as_array()
            .unwrap()
            .iter()
            .all(|r| r["literal_check"] == false)
    );
    assert!(option1::carry::previous(saved, head, 2).is_some());
    assert_eq!(
        kah_cli::observer_review::reminder_queue::pending(directory)
            .unwrap()
            .len(),
        1
    );
    assert!(!directory.join("option1-checkpoint-baseline.json").exists());
}
