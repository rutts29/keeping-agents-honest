use kah_cli::observer_review::{
    queue,
    state::{Purpose, Store},
};
use serde_json::json;

fn input(record: &str, text: &str) -> serde_json::Value {
    json!({"uncertain_user_input":{"source":{"provider":"codex","session":"worker","record":record,"role":"user","content_digest":"sha256:fixture"},"text":text,"decision":null,"dismissed":false,"promoted":false}})
}

#[test]
fn identical_steering_is_one_entry_across_turns_and_after_delivery() {
    let worker = format!("steering-dedupe-{}", std::process::id());
    let root = Store::directory(&worker).unwrap();
    for turn in ["first", "second", "third"] {
        queue::enqueue(
            &worker,
            Purpose::Steering,
            turn,
            input("inherited:real-duplicate", "Whole instruction."),
        )
        .unwrap();
    }
    let jobs = queue::entries(&worker).unwrap();
    assert_eq!(
        jobs.len(),
        1,
        "same inherited source must not create a new review every hook"
    );
    let covered = root.join("covered-triggers");
    kah_core::run_store::ensure_private_directory(&covered).unwrap();
    std::fs::rename(&jobs[0].0, covered.join(jobs[0].0.file_name().unwrap())).unwrap();
    queue::enqueue(
        &worker,
        Purpose::Steering,
        "fourth",
        input("inherited:real-duplicate", "Whole instruction."),
    )
    .unwrap();
    assert!(queue::entries(&worker).unwrap().is_empty());
    queue::enqueue(
        &worker,
        Purpose::Steering,
        "new",
        input("new-user-receipt", "Whole instruction."),
    )
    .unwrap();
    queue::enqueue(
        &worker,
        Purpose::Steering,
        "edited",
        input("inherited:real-duplicate", "Changed whole instruction."),
    )
    .unwrap();
    assert_eq!(
        queue::entries(&worker).unwrap().len(),
        2,
        "different authority or content remains separate"
    );
    std::fs::remove_dir_all(root.parent().unwrap()).unwrap();
}

#[test]
fn drift_merges_at_enqueue_and_post_snapshot_actions_get_a_followup() {
    let worker = format!("drift-enqueue-{}", std::process::id());
    let root = Store::directory(&worker).unwrap();
    for id in ["a", "b"] {
        queue::enqueue(
            &worker,
            Purpose::Drift,
            id,
            json!({"actions":[{"id":id,"output":"Whole output.\n".repeat(10000)}]}),
        )
        .unwrap();
    }
    let before = queue::entries(&worker).unwrap();
    assert_eq!(
        before.len(),
        1,
        "coalescing happens even while checkpoints block draining"
    );
    assert_eq!(before[0].1.trigger["actions"].as_array().unwrap().len(), 2);
    // Simulate a crash after durable pending publication, before original archive.
    let missing = lose_original(&root);
    let (snapshot, paths) = queue::claim_batch(&worker).unwrap().unwrap();
    assert!(
        missing.exists(),
        "claim restores the whole missing original from the durable packet"
    );
    assert!(!paths[0].ends_with("drift-pending.json"));
    queue::enqueue(
        &worker,
        Purpose::Drift,
        "after",
        json!({"actions":[{"id":"after"}]}),
    )
    .unwrap();
    let jobs = queue::entries(&worker).unwrap();
    assert_eq!(
        jobs.len(),
        2,
        "the claimed review and post-snapshot pending packet stay distinct"
    );
    assert_eq!(snapshot.trigger["actions"].as_array().unwrap().len(), 2);
    assert_eq!(
        jobs.iter()
            .find(|(p, _)| p.ends_with("drift-pending.json"))
            .unwrap()
            .1
            .trigger["actions"][0]["id"],
        "after"
    );
    assert_eq!(
        std::fs::read_dir(root.join("drift-events"))
            .unwrap()
            .count(),
        3,
        "every original flag is retained"
    );
    std::fs::remove_dir_all(root.parent().unwrap()).unwrap();
}
fn lose_original(root: &std::path::Path) -> std::path::PathBuf {
    let path = std::fs::read_dir(root.join("drift-events"))
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path();
    std::fs::remove_file(&path).unwrap();
    path
}

#[test]
fn one_time_backlog_repair_keeps_every_original_and_is_idempotent() {
    let worker = format!("backlog-repair-{}", std::process::id());
    let root = Store::directory(&worker).unwrap();
    kah_core::run_store::ensure_private_directory(&root.join("queued")).unwrap();
    kah_core::run_store::ensure_private_directory(&root.join("sources")).unwrap();
    for (i, purpose) in [
        Purpose::Steering,
        Purpose::Steering,
        Purpose::Drift,
        Purpose::Drift,
        Purpose::Preparation,
        Purpose::Checkpoint,
    ]
    .into_iter()
    .enumerate()
    {
        let trigger = if purpose == Purpose::Steering {
            input("inherited:old-loop", "Whole input.")
        } else {
            json!({"actions":[{"id":i}],"whole":"Whole source.\n".repeat(10000)})
        };
        let wake = queue::Wake {
            purpose,
            turn: i.to_string(),
            trigger,
        };
        kah_cli::observer_review::source_store::save(
            &root.join("queued"),
            &format!("old-{i}.json"),
            &serde_json::to_value(wake).unwrap(),
        )
        .unwrap();
    }
    let latest = queue::Wake {
        purpose: Purpose::Checkpoint,
        turn: "latest".into(),
        trigger: json!({"newest_head":"review-the-latest"}),
    };
    let repaired = queue::repair::backlog(&worker, "user-authorized-once", latest.clone()).unwrap();
    assert_eq!(repaired["before"], 6);
    assert_eq!(repaired["after"], 3);
    assert_eq!(
        std::fs::read_dir(root.join("queue-retired"))
            .unwrap()
            .count(),
        6
    );
    verify_retained_rows(&root, &repaired);
    assert_eq!(
        queue::repair::backlog(&worker, "user-authorized-once", latest).unwrap(),
        repaired
    );
    std::fs::remove_dir_all(root.parent().unwrap()).unwrap();
}
fn verify_retained_rows(root: &std::path::Path, repaired: &serde_json::Value) {
    for row in repaired["rows"].as_array().unwrap() {
        let original: queue::Wake = kah_cli::observer_review::source_store::read(
            &root
                .join("queue-retired")
                .join(row["original"].as_str().unwrap()),
        )
        .unwrap();
        if original.purpose == Purpose::Steering {
            assert_eq!(
                original.trigger["uncertain_user_input"]["text"],
                "Whole input."
            );
        } else {
            assert_eq!(original.trigger["whole"], "Whole source.\n".repeat(10000));
        }
        assert!(row["replacement"].as_str().is_some());
    }
}
