use kah_cli::observer_review::reminder_queue;
use serde_json::json;
#[test]
fn later_findings_cannot_overwrite_an_undelivered_checkpoint() {
    let root = std::env::temp_dir().join(format!("kah-reminder-queue-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let first = json!({"request":"checkpoint-1","purpose":"checkpoint","text":"Checkpoint on SHA1 needs integration"});
    let second =
        json!({"request":"steering-2","purpose":"steering","text":"Apply the user's correction"});
    reminder_queue::save(&root, &first).unwrap();
    reminder_queue::save(&root, &second).unwrap();
    let (path, value) = reminder_queue::next(&root).unwrap().unwrap();
    assert_eq!(value["request"], "checkpoint-1");
    std::fs::remove_file(path).unwrap();
    assert_eq!(
        reminder_queue::next(&root).unwrap().unwrap().1["request"],
        "steering-2"
    );
    std::fs::remove_dir_all(root).unwrap();
}
#[test]
fn legacy_migration_and_same_request_revisions_are_lossless_and_do_not_duplicate() {
    let root = std::env::temp_dir().join(format!("kah-reminder-migrate-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(
        root.join("reminder.json"),
        serde_json::to_vec(&json!({"request":"r","purpose":"drift","text":"original whole text"}))
            .unwrap(),
    )
    .unwrap();
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(
        root.join("reminder.json"),
        std::fs::Permissions::from_mode(0o600),
    )
    .unwrap();
    assert_eq!(
        reminder_queue::next(&root).unwrap().unwrap().1["text"],
        "original whole text"
    );
    assert!(!root.join("reminder.json").exists());
    reminder_queue::save(
        &root,
        &json!({"request":"r","purpose":"drift","text":"revised complete reminder"}),
    )
    .unwrap();
    let (path, value) = reminder_queue::next(&root).unwrap().unwrap();
    assert_eq!(value["text"], "revised complete reminder");
    std::fs::remove_file(path).unwrap();
    assert!(reminder_queue::next(&root).unwrap().is_none());
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn legacy_checkpoint_keeps_its_original_order_when_new_code_already_queued_steering() {
    let root =
        std::env::temp_dir().join(format!("kah-reminder-legacy-order-{}", std::process::id()));
    kah_core::run_store::ensure_private_directory(&root).unwrap();
    kah_cli::unattended_report::save(
        &root,
        "reminder.json",
        &json!({"request":"old","purpose":"checkpoint","text":"Original checkpoint"}),
    )
    .unwrap();
    reminder_queue::save(
        &root,
        &json!({"request":"new","purpose":"steering","text":"Later steering"}),
    )
    .unwrap();
    assert_eq!(
        reminder_queue::next(&root).unwrap().unwrap().1["request"],
        "old"
    );
    std::fs::remove_dir_all(root).unwrap();
}
