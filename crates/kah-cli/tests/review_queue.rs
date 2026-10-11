use kah_cli::observer_review::{
    queue,
    state::{Purpose, Store},
};
use serde_json::json;
use std::fs;

#[test]
fn busy_reviews_preserve_each_commit_and_whole_preparation_across_turns() {
    let worker = format!("queued-unit-{}", std::process::id());
    let store = Store::open(&worker).unwrap();
    let whole = json!({"state":{"user_text":"Whole relayed instruction.\n".repeat(10000)}});
    queue::enqueue(
        &worker,
        Purpose::Checkpoint,
        "same-turn",
        json!({"marker":"first"}),
    )
    .unwrap();
    queue::enqueue(&worker, Purpose::Preparation, "same-turn", whole.clone()).unwrap();
    queue::enqueue(
        &worker,
        Purpose::Checkpoint,
        "later-turn",
        json!({"marker":"latest"}),
    )
    .unwrap();
    let jobs = queue::entries(&worker).unwrap();
    assert_eq!(jobs.len(), 3);
    for expected in [json!({"marker":"first"}), json!({"marker":"latest"}), whole] {
        assert!(jobs.iter().any(|(_, j)| j.trigger == expected));
    }
    let directory = store.directory.clone();
    drop(store);
    fs::remove_dir_all(directory.parent().unwrap()).unwrap();
}

#[test]
fn uncertain_steering_cannot_discard_simultaneous_source_preparation() {
    let preparation = json!({"preparation":{"state":{"user_text":"whole user words"}}});
    let steering = json!({"uncertain_user_input":{"text":"whole user instruction"}});
    let triggers = kah_cli::observer_review::dispatch::triggers(
        Some(preparation.clone()),
        Some(steering.clone()),
    );
    assert_eq!(
        triggers,
        vec![
            (Purpose::Steering, steering),
            (Purpose::Preparation, preparation)
        ]
    );
}

#[test]
fn commit_snapshots_keep_their_own_work_and_later_events_remain_queued() {
    let worker = format!("commit-queue-unit-{}", std::process::id());
    let store = Store::open(&worker).unwrap();
    queue::enqueue(&worker, Purpose::Checkpoint, "one", json!({"marker":"a"})).unwrap();
    queue::enqueue(&worker, Purpose::Checkpoint, "two", json!({"marker":"b"})).unwrap();
    let (job, paths) = queue::claim_batch(&worker).unwrap().unwrap();
    assert_eq!(paths.len(), 1);
    assert_eq!(job.trigger["marker"], "a");
    queue::enqueue(&worker, Purpose::Checkpoint, "three", json!({"marker":"c"})).unwrap();
    for path in paths {
        fs::remove_file(path).unwrap();
    }
    let remaining = queue::entries(&worker).unwrap();
    assert_eq!(remaining.len(), 2);
    assert!(remaining.iter().any(|(_, j)| j.trigger["marker"] == "b"));
    assert!(remaining.iter().any(|(_, j)| j.trigger["marker"] == "c"));
    let directory = store.directory.clone();
    drop(store);
    fs::remove_dir_all(directory.parent().unwrap()).unwrap();
}

#[test]
fn a_busy_startup_store_preserves_the_trigger_and_continues_after_release() {
    let worker = format!("busy-wake-unit-{}", std::process::id());
    let store = Store::open(&worker).unwrap();
    save_unreachable_state(&store, &worker);
    queue::enqueue(
        &worker,
        Purpose::Checkpoint,
        "startup",
        json!({"actions":[{"id":"first"}]}),
    )
    .unwrap();
    let name = worker.clone();
    let (thread, result) = blocked_admission(move || queue::drain(&name));
    assert_eq!(queue::entries(&worker).unwrap().len(), 1);
    let directory = store.directory.clone();
    drop(store);
    assert!(
        result.recv().unwrap().is_err(),
        "must advance to transport after release, never stop as queued"
    );
    thread.join().unwrap();
    fs::remove_dir_all(directory.parent().unwrap()).unwrap();
}

#[test]
fn captured_snapshots_are_queued_whole_and_never_coalesced_away() {
    use kah_cli::observer_review::{snapshot, state::Request};
    let worker = format!("snapshot-busy-unit-{}", std::process::id());
    let store = Store::open(&worker).unwrap();
    save_unreachable_state(&store, &worker);
    let request = Request {
        id: "review-original".into(),
        worker: worker.clone(),
        purpose: Purpose::Checkpoint,
        turn: "historical".into(),
        head: None,
        intent: json!({"goal":"Original whole user words."}),
        trigger: json!({"chronology":"unchanged"}),
        documents: vec![],
        created_ms: 1,
    };
    let copied = request.clone();
    let (thread, result) = blocked_admission(move || snapshot::wake(&copied));
    queue::enqueue(
        &worker,
        Purpose::Checkpoint,
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
    assert!(
        result.recv().unwrap().is_err(),
        "captured admission must continue after release"
    );
    thread.join().unwrap();
    fs::remove_dir_all(directory.parent().unwrap()).unwrap();
}

fn save_unreachable_state(store: &Store, worker: &str) {
    let workspace = store.directory.join("observer");
    let value = json!({"worker":{"provider":"codex","session":worker,"project":workspace,"pane":null,"socket":null},
        "observer":{"session":null,"pane":"absent","name":"fixture","workspace":workspace,"model":"gpt-6.1-sol","effort":"low","transcript":null},
        "pending":null,"latest":null,"last_verified_turn":null,"checkpoint":null,"deferrals":[],"wakes":0,"native_tokens":0,"stopped":false});
    store.save(&serde_json::from_value(value).unwrap()).unwrap();
}

type AdmissionResult = Result<serde_json::Value, String>;
fn blocked_admission(
    run: impl FnOnce() -> kah_cli::Result<serde_json::Value> + Send + 'static,
) -> (
    std::thread::JoinHandle<()>,
    std::sync::mpsc::Receiver<AdmissionResult>,
) {
    let (tid_send, tid_recv) = std::sync::mpsc::channel();
    let (send, recv) = std::sync::mpsc::channel();
    let thread = std::thread::spawn(move || {
        // Linux fixture: observe actual flock waiting, not a timing assumption.
        tid_send
            .send(fs::read_link("/proc/thread-self").unwrap())
            .unwrap();
        send.send(run().map_err(|e| e.to_string())).unwrap();
    });
    let tid = tid_recv.recv().unwrap();
    loop {
        if let Ok(result) = recv.try_recv() {
            panic!("admission returned while the store was still held: {result:?}");
        }
        let wait = fs::read_to_string(std::path::Path::new("/proc").join(&tid).join("wchan"))
            .unwrap_or_default();
        if wait.trim() == "locks_lock_inode_wait" {
            break;
        }
        std::thread::yield_now();
    }
    (thread, recv)
}

#[test]
fn gate_precedes_fair_lane_service_with_every_item_and_sha_delivered_once() {
    let worker = format!("priority-checkpoint-unit-{}", std::process::id());
    let store = Store::open(&worker).unwrap();
    let cases = [
        (Purpose::Steering, "intent-a", ""),
        (
            Purpose::Checkpoint,
            "commit-a",
            "63700d34fd9fe967b1bed5b7f9a2971aaa1db5a7",
        ),
        (Purpose::Checkpoint, "gate", ""),
        (Purpose::Steering, "intent-b", ""),
        (
            Purpose::Checkpoint,
            "commit-b",
            "d15e9f532a44948fe842e01e239a068d0367a80a",
        ),
    ];
    for (purpose, id, sha) in &cases {
        let mut trigger = json!({"id":id,"whole":"Whole exact source.\n","commit_sha":sha});
        if id.starts_with("commit") {
            trigger["review_class"] = json!("commit");
        }
        queue::enqueue(&worker, purpose.clone(), "turn", trigger).unwrap();
    }
    // Cross-lane FIFO belonged to the deleted Drift/Intent lane. Keep gate-first,
    // FIFO within each current lane, exact content/SHA and no starvation instead.
    let mut served = vec![];
    while let Some((job, paths)) = queue::batch(&queue::entries(&worker).unwrap()) {
        assert_eq!(paths.len(), 1);
        assert_eq!(job.trigger["whole"], "Whole exact source.\n");
        served.push(job.trigger.clone());
        for path in paths {
            fs::remove_file(path).unwrap();
        }
    }
    assert_eq!(served[0]["id"], "gate");
    assert_eq!(served.len(), cases.len());
    for (_, id, sha) in &cases {
        let rows = served.iter().filter(|v| v["id"] == *id).collect::<Vec<_>>();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0]["commit_sha"], *sha);
    }
    for lane in ["intent", "commit"] {
        let ids = served
            .iter()
            .filter_map(|v| v["id"].as_str())
            .filter(|id| id.starts_with(lane))
            .collect::<Vec<_>>();
        assert_eq!(
            ids,
            vec![format!("{lane}-a"), format!("{lane}-b")],
            "both lanes served in the same run, each FIFO"
        );
    }
    let directory = store.directory.clone();
    drop(store);
    fs::remove_dir_all(directory.parent().unwrap()).unwrap();
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
