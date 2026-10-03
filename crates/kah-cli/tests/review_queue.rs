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

#[test]
fn uncertain_steering_cannot_discard_a_simultaneous_drift_trigger() {
    let drift = json!({"verified_actions":["whole drift evidence"]});
    let steering = json!({"uncertain_user_input":{"text":"whole user instruction"}});
    let triggers =
        kah_cli::observer_review::dispatch::triggers(Some(drift.clone()), Some(steering.clone()));
    assert_eq!(triggers.len(), 2);
    assert_eq!(triggers[0], (Purpose::Steering, steering));
    assert_eq!(triggers[1], (Purpose::Drift, drift));
}

#[test]
fn uncertain_ticks_coalesce_at_snapshot_and_later_events_remain_queued() {
    let worker = format!("coalesced-unit-{}", std::process::id());
    let store = Store::open(&worker).unwrap();
    queue::enqueue(
        &worker,
        Purpose::Drift,
        "turn-one",
        json!({"actions":[{"id":"a"}],"tripwire":{"assessment":"uncertain"}}),
    )
    .unwrap();
    queue::enqueue(
        &worker,
        Purpose::Drift,
        "turn-two",
        json!({"actions":[{"id":"b"}],"tripwire":{"assessment":"uncertain"}}),
    )
    .unwrap();
    let snapshot = queue::entries(&worker).unwrap();
    let (job, paths) = queue::batch(&snapshot).unwrap();
    assert_eq!(paths.len(), 2);
    assert_eq!(job.trigger["actions"].as_array().unwrap().len(), 2);
    assert_eq!(
        job.trigger["coalesced_triggers"].as_array().unwrap().len(),
        2
    );
    queue::enqueue(
        &worker,
        Purpose::Drift,
        "turn-three",
        json!({"actions":[{"id":"c"}]}),
    )
    .unwrap();
    for path in paths {
        fs::remove_file(path).unwrap();
    }
    assert_eq!(
        queue::entries(&worker).unwrap()[0].1.trigger["actions"][0]["id"],
        "c"
    );
    let directory = store.directory.clone();
    drop(store);
    fs::remove_dir_all(directory.parent().unwrap()).unwrap();
}
