//! Replay the retained empty completion report after an unconfirmed tag.
use kah_cli::observer_review::{
    option1, source_store,
    state::{Pending, Purpose, Request, State, Store},
};
use serde_json::{Value, json};
use std::fs;

#[test]
fn retained_empty_range_carries_prior_rows_instead_of_turning_all_eighteen_unclear() {
    std::env::var_os("XDG_STATE_HOME").expect("run through demos/check-tests.sh");
    let replay: Value = serde_json::from_str(include_str!(
        "../../../docs/gates/evidence/phase3-claude/b2/same-sha-terminal/checkpoint-replay.json"
    ))
    .unwrap();
    let worker = format!("empty-carry-{}", kah_cli::relay_store::nonce());
    let root = std::env::temp_dir().join(&worker);
    fs::create_dir_all(root.join("outbox")).unwrap();
    let store = Store::open(&worker).unwrap();
    source_store::initialize(&store.directory, &root).unwrap();
    let directory = store.directory.clone();
    let tag = &replay["cases"][1];
    let end = &replay["cases"][2];
    let request = Request {
        id: end["id"].as_str().unwrap().into(),
        worker: worker.clone(),
        purpose: Purpose::Checkpoint,
        turn: "recorded-end".into(),
        head: Some(replay["head"].as_str().unwrap().into()),
        intent: json!({"revision":2}),
        documents: vec![],
        created_ms: 1,
        trigger: json!({"option1":{"stage":"checkpoint","input":end["input"],"original_trigger":end["trigger"]}}),
    };
    let checkpoint = json!({"head":replay["head"],"intent_revision":2,"request":tag["id"],"passed":false,"result":tag["latest"]});
    let mut state:State=serde_json::from_value(json!({"worker":{"provider":"claude","session":worker,"project":root,"pane":null,"socket":null},
        "observer":{"session":"recorded-observer","pane":null,"name":"recorded-observer","workspace":root,"model":"claude-sonnet-5-5","effort":"low","transcript":null},
        "pending":null,"latest":tag["latest"],"last_verified_turn":null,"checkpoint":checkpoint,"deferrals":[],"wakes":0,"native_tokens":0,"stopped":false})).unwrap();
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
    assert_eq!(
        saved.checkpoint.unwrap()["result"]["rows"],
        tag["report"]["result"]["rows"]
    );
    assert!(
        kah_cli::observer_review::reminder_queue::pending(&directory)
            .unwrap()
            .is_empty()
    );
    fs::remove_dir_all(directory.parent().unwrap()).unwrap();
    fs::remove_dir_all(root).unwrap();
}
