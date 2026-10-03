use kah_cli::observer_review::{
    queue,
    state::{Purpose, Store},
};
use serde_json::json;
use std::fs;

#[test]
fn busy_reviews_preserve_every_trigger_and_whole_preparation_packet_across_turns() {
    let worker = format!("queued-unit-{}", std::process::id());
    let store = Store::open(&worker).unwrap();
    let whole = json!({"state":{"user_text":"Whole relayed instruction.\n".repeat(10000)}});
    queue::enqueue(
        &worker,
        Purpose::Drift,
        "same-turn",
        json!({"marker":"first"}),
    )
    .unwrap();
    queue::enqueue(&worker, Purpose::Preparation, "same-turn", whole.clone()).unwrap();
    queue::enqueue(
        &worker,
        Purpose::Drift,
        "later-turn",
        json!({"marker":"latest"}),
    )
    .unwrap();
    let jobs = queue::entries(&worker).unwrap();
    assert_eq!(jobs.len(), 3);
    assert_eq!(jobs[0].1.turn, "same-turn");
    assert_eq!(jobs[1].1.trigger, whole);
    assert_eq!(jobs[2].1.trigger["marker"], "latest");
    let directory = store.directory.clone();
    drop(store);
    fs::remove_dir_all(directory.parent().unwrap()).unwrap();
}
