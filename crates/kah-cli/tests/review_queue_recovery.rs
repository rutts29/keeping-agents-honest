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
fn commit_metadata_remains_separate_and_post_snapshot_work_is_retained() {
    let worker = format!("commit-recovery-{}", std::process::id());
    let root = Store::directory(&worker).unwrap();
    for id in ["a", "b"] {
        queue::enqueue(
            &worker,
            Purpose::Checkpoint,
            id,
            json!({"actions":[{"id":id,"output":"Whole output.\n".repeat(10000)}]}),
        )
        .unwrap();
    }
    assert_eq!(queue::entries(&worker).unwrap().len(), 2);
    let (snapshot, paths) = queue::claim_batch(&worker).unwrap().unwrap();
    assert_eq!(paths.len(), 1);
    assert_eq!(snapshot.trigger["actions"].as_array().unwrap().len(), 1);
    queue::enqueue(
        &worker,
        Purpose::Checkpoint,
        "after",
        json!({"actions":[{"id":"after"}]}),
    )
    .unwrap();
    for path in paths {
        std::fs::remove_file(path).unwrap();
    }
    let jobs = queue::entries(&worker).unwrap();
    assert_eq!(jobs.len(), 2);
    assert!(
        jobs.iter()
            .any(|(_, w)| w.trigger["actions"][0]["id"] == "after")
    );
    std::fs::remove_dir_all(root.parent().unwrap()).unwrap();
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
    assert_eq!(repaired["after"], 2);
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
