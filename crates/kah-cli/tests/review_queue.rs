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
    // Merge drift at enqueue while preserving all content.
    assert_eq!(jobs.len(), 2);
    assert_eq!(jobs[0].1.turn, "later-turn");
    assert_eq!(
        jobs[0].1.trigger["coalesced_triggers"][0]["marker"],
        "first"
    );
    assert_eq!(
        jobs[0].1.trigger["coalesced_triggers"][1]["marker"],
        "latest"
    );
    assert_eq!(jobs[1].1.trigger, whole);
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
    // Same user rule: claiming detaches the single pending aggregate.
    let (job, paths) = queue::claim_batch(&worker).unwrap().unwrap();
    assert_eq!(paths.len(), 1);
    assert_eq!(job.trigger["actions"].as_array().unwrap().len(), 2);
    assert_eq!(
        snapshot[0].1.trigger["coalesced_triggers"]
            .as_array()
            .unwrap()
            .len(),
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

#[test]
fn a_busy_startup_store_preserves_the_trigger_without_failing_the_wake() {
    let worker = format!("busy-wake-unit-{}", std::process::id());
    let store = Store::open(&worker).unwrap();
    queue::enqueue(
        &worker,
        Purpose::Drift,
        "startup",
        json!({"actions":[{"id":"first"}]}),
    )
    .unwrap();
    let receipt = queue::drain(&worker).unwrap();
    assert_eq!(receipt["status"], "queued");
    assert_eq!(queue::entries(&worker).unwrap().len(), 1);
    let directory = store.directory.clone();
    drop(store);
    fs::remove_dir_all(directory.parent().unwrap()).unwrap();
}

#[test]
fn captured_snapshots_are_queued_whole_and_never_coalesced_away() {
    use kah_cli::observer_review::{snapshot, state::Request};
    let worker = format!("snapshot-busy-unit-{}", std::process::id());
    let store = Store::open(&worker).unwrap();
    let request = Request {
        id: "review-original".into(),
        worker: worker.clone(),
        purpose: Purpose::Drift,
        turn: "historical".into(),
        head: None,
        intent: json!({"goal":"Original whole user words."}),
        trigger: json!({"chronology":"unchanged"}),
        documents: vec![],
        created_ms: 1,
    };
    let receipt = snapshot::wake(&request).unwrap();
    assert_eq!(receipt["status"], "queued");
    queue::enqueue(
        &worker,
        Purpose::Drift,
        "next",
        json!({"actions":[{"id":"next"}]}),
    )
    .unwrap();
    let jobs = queue::entries(&worker).unwrap();
    let (first, paths) = queue::batch(&jobs).unwrap();
    assert_eq!(paths.len(), 1);
    assert_eq!(first.trigger["captured_snapshot"]["intent"], request.intent);
    assert_eq!(jobs.len(), 2);
    let directory = store.directory.clone();
    drop(store);
    fs::remove_dir_all(directory.parent().unwrap()).unwrap();
}

#[test]
fn explicit_checkpoint_precedes_rechecks_without_discarding_or_reordering_the_rest() {
    let worker = format!("priority-checkpoint-unit-{}", std::process::id());
    let store = Store::open(&worker).unwrap();
    for (purpose, id) in [
        (Purpose::Steering, "steering-first"),
        (Purpose::Drift, "drift-second"),
        (Purpose::Checkpoint, "gate-third"),
        (Purpose::Steering, "steering-last"),
    ] {
        queue::enqueue(&worker, purpose, "turn", json!({"id":id})).unwrap();
    }
    let jobs = queue::entries(&worker).unwrap();
    assert_eq!(jobs.len(), 4);
    assert_eq!(jobs[0].1.purpose, Purpose::Checkpoint);
    assert_eq!(jobs[1].1.trigger["id"], "steering-first");
    assert_eq!(jobs[2].1.trigger["id"], "drift-second");
    assert_eq!(jobs[3].1.trigger["id"], "steering-last");
    let (next, paths) = queue::batch(&jobs).unwrap();
    assert_eq!(next.trigger["id"], "gate-third");
    assert_eq!(paths.len(), 1);
    std::fs::remove_file(&paths[0]).unwrap();
    let remaining = queue::entries(&worker).unwrap();
    assert_eq!(remaining.len(), 3);
    assert_eq!(remaining[0].1.trigger["id"], "steering-first");
    let directory = store.directory.clone();
    drop(store);
    std::fs::remove_dir_all(directory.parent().unwrap()).unwrap();
}

#[test]
fn checkpoint_events_transfer_before_rechecks_and_status_exposes_the_order() {
    use kah_cli::observer_review::{control, events};
    let worker = format!("checkpoint-transfer-unit-{}", std::process::id());
    let store = Store::open(&worker).unwrap();
    queue::enqueue(&worker, Purpose::Steering, "turn", json!({"id":"routine"})).unwrap();
    let directory = store.directory.join("checkpoint-events");
    kah_core::run_store::ensure_private_directory(&directory).unwrap();
    kah_cli::unattended_report::save(&directory, "claim.json", &json!({"kind":"tag"})).unwrap();
    assert!(events::pending(&worker).unwrap());
    events::transfer(&worker).unwrap();
    events::transfer(&worker).unwrap();
    assert!(!events::pending(&worker).unwrap());
    let jobs = queue::entries(&worker).unwrap();
    assert_eq!(jobs.len(), 2);
    assert_eq!(jobs[0].1.trigger["events"][0]["kind"], "tag");
    assert_eq!(jobs[1].1.trigger["id"], "routine");
    let status = control::status(&worker).unwrap();
    assert_eq!(status["queued"][0]["purpose"], "checkpoint");
    assert_eq!(status["queued"][1]["purpose"], "steering");
    assert_eq!(status["checkpoint_events_pending"], false);
    let root = store.directory.parent().unwrap().to_path_buf();
    drop(store);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn final_verification_review_precedes_routine_checkpoints_without_losing_any() {
    let worker = format!("verification-final-priority-{}", std::process::id());
    let store = Store::open(&worker).unwrap();
    queue::enqueue(
        &worker,
        Purpose::Checkpoint,
        "old",
        json!({"events":[{"kind":"native_phase_claim"}]}),
    )
    .unwrap();
    queue::enqueue(
        &worker,
        Purpose::Checkpoint,
        "checks",
        json!({"verification_followup":{"stage":"checks_complete","round":"one"}}),
    )
    .unwrap();
    queue::enqueue(
        &worker,
        Purpose::Checkpoint,
        "new",
        json!({"events":[{"kind":"native_phase_claim"}]}),
    )
    .unwrap();
    let jobs = queue::entries(&worker).unwrap();
    assert_eq!(jobs.len(), 3);
    assert_eq!(jobs[0].1.turn, "checks");
    assert_eq!(jobs[1].1.turn, "old");
    assert_eq!(jobs[2].1.turn, "new");
    let directory = store.directory.clone();
    drop(store);
    std::fs::remove_dir_all(directory.parent().unwrap()).unwrap();
}

#[test]
fn receipt_and_nonce_entries_follow_publish_chronology_not_filename_order() {
    let worker = format!("queue-time-order-{}", std::process::id());
    let store = Store::open(&worker).unwrap();
    let wake = queue::Wake {
        purpose: Purpose::Checkpoint,
        turn: "earlier-rethink".into(),
        trigger: json!({}),
    };
    queue::enqueue_once_at(&store.directory, "earlier-rethink", &wake).unwrap();
    queue::enqueue(&worker, Purpose::Checkpoint, "later-claim", json!({})).unwrap();
    for entry in std::fs::read_dir(store.directory.join("queued")).unwrap() {
        let path = entry.unwrap().path();
        let earlier = path
            .file_name()
            .unwrap()
            .to_str()
            .unwrap()
            .starts_with("receipt-");
        let modified =
            std::time::UNIX_EPOCH + std::time::Duration::from_secs(if earlier { 1 } else { 2 });
        std::fs::File::open(path)
            .unwrap()
            .set_times(std::fs::FileTimes::new().set_modified(modified))
            .unwrap();
    }
    let jobs = queue::entries(&worker).unwrap();
    assert_eq!(jobs[0].1.turn, "earlier-rethink");
    assert_eq!(jobs[1].1.turn, "later-claim");
    let directory = store.directory.clone();
    drop(store);
    std::fs::remove_dir_all(directory.parent().unwrap()).unwrap();
}

#[test]
fn recorded_fifo_survives_timestamp_reversal_and_ignores_publisher_temporaries() {
    let worker = format!("queue-durable-order-{}", std::process::id());
    let store = Store::open(&worker).unwrap();
    queue::enqueue_once_at(
        &store.directory,
        "first",
        &queue::Wake {
            purpose: Purpose::Checkpoint,
            turn: "first".into(),
            trigger: json!({}),
        },
    )
    .unwrap();
    queue::enqueue(&worker, Purpose::Checkpoint, "second", json!({})).unwrap();
    for entry in fs::read_dir(store.directory.join("queued")).unwrap() {
        let path = entry.unwrap().path();
        let receipt = path
            .file_name()
            .unwrap()
            .to_str()
            .unwrap()
            .starts_with("receipt-");
        let time =
            std::time::UNIX_EPOCH + std::time::Duration::from_secs(if receipt { 20 } else { 1 });
        fs::File::open(path)
            .unwrap()
            .set_times(fs::FileTimes::new().set_modified(time))
            .unwrap();
    }
    std::os::unix::fs::symlink(
        "missing-publisher",
        store.directory.join("queued/report-moving.tmp"),
    )
    .unwrap();
    let jobs = queue::entries(&worker).unwrap();
    assert_eq!(
        jobs.iter()
            .map(|(_, w)| w.turn.as_str())
            .collect::<Vec<_>>(),
        vec!["first", "second"]
    );
    let directory = store.directory.clone();
    drop(store);
    fs::remove_dir_all(directory.parent().unwrap()).unwrap();
}
