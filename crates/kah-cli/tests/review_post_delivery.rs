//! Retained main-review race: enqueue supersedes a claimed file during capture.
use kah_cli::observer_review::queue;
use std::{fs, path::PathBuf};

#[test]
fn a_delivered_trigger_already_retained_by_supersession_is_not_a_failure() {
    let root = fixture("superseded");
    let queued = root.join("queued/original.json");
    let retired = root.join("queue-retired/original.json");
    fs::write(&queued, b"whole claimed trigger").unwrap();
    fs::rename(&queued, &retired).unwrap();
    let newer = root.join("queued/newer.json");
    fs::write(&newer, b"latest includes original claims and authority").unwrap();
    queue::archive_submitted(&root, &[queued]).unwrap();
    assert_eq!(fs::read(retired).unwrap(), b"whole claimed trigger");
    assert_eq!(
        fs::read(newer).unwrap(),
        b"latest includes original claims and authority"
    );
    fs::remove_dir_all(root).unwrap();
}
fn fixture(label: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!("post-delivery-{label}-{}", std::process::id()));
    for dir in ["queued", "queue-retired", "covered-triggers"] {
        fs::create_dir_all(root.join(dir)).unwrap();
    }
    root
}

#[test]
fn repeated_transfer_is_idempotent_and_unrecorded_loss_has_a_named_path() {
    let root = fixture("repeated");
    let queued = root.join("queued/original.json");
    fs::write(&queued, b"whole claimed trigger").unwrap();
    queue::archive_submitted(&root, std::slice::from_ref(&queued)).unwrap();
    queue::archive_submitted(&root, std::slice::from_ref(&queued)).unwrap();
    assert_eq!(
        fs::read(root.join("covered-triggers/original.json")).unwrap(),
        b"whole claimed trigger"
    );
    let missing = root.join("queued/not-retained.json");
    let error = queue::archive_submitted(&root, std::slice::from_ref(&missing))
        .unwrap_err()
        .to_string();
    assert!(error.contains("REVIEW_QUEUE_TRANSFER_IO stage=queue_transfer"));
    assert!(error.contains(missing.to_str().unwrap()));
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn resolving_success_keeps_newer_gaps_and_archives_the_entire_old_diagnostic() {
    use kah_cli::{
        observer_review::{consume, source_store},
        unattended_report,
    };
    use serde_json::{Value, json};
    let root = fixture("gaps");
    fs::create_dir(root.join("sources")).unwrap();
    let old = json!({"request":"review-old","error":"REVIEW_IO stage=review: missing file","retained":"whole exact diagnostic"});
    let newer = json!({"request":"review-new","error":"newer request still needs evidence"});
    unattended_report::save(&root, "gap.json", &old).unwrap();
    unattended_report::save(&root, "delivered-gap.json", &old).unwrap();
    unattended_report::save(&root, "validation-current.json", &newer).unwrap();
    consume::resolve_gaps(&root, "review-old").unwrap();
    assert!(!root.join("gap.json").exists());
    assert!(!root.join("delivered-gap.json").exists());
    assert_eq!(
        source_store::read::<Value>(&root.join("validation-current.json")).unwrap(),
        newer
    );
    let records = fs::read_dir(root.join("resolved-gaps"))
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    assert_eq!(records.len(), 2);
    for record in records {
        assert_eq!(
            source_store::read::<Value>(&record.path()).unwrap()["original"],
            old
        );
    }
    consume::resolve_gaps(&root, "review-old").unwrap();
    fs::remove_dir_all(root).unwrap();
}
