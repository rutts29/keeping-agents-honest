use super::*;

#[test]
fn initial_host_expansion_is_recorded_without_a_user_notice() {
    let root = std::env::temp_dir().join(format!(
        "kah-quiet-expansion-{}",
        crate::relay_store::nonce()
    ));
    kah_core::run_store::ensure_private_directory(&root).unwrap();
    let progress = Progress {
        sample: Sample {
            request: "review-source".into(),
            now_ms: 0,
            started_ms: 0,
            native_active: false,
            controller_active: true,
            transcript_version: None,
            report_version: None,
            cause: "HOST EXPANSION: REVIEW_SOURCE_OUTSIDE_PACKET stage=report".into(),
        },
        last_progress_ms: 0,
        stalled: false,
        changed: true,
    };
    event(&root, "disposable-worker", &progress).unwrap();
    assert!(!root.join("notice.json").exists());
    assert!(root.join("events.jsonl").is_file());
    std::fs::remove_dir_all(root).unwrap();
}
#[test]
fn current_post_ack_validation_is_distinguished_from_delivery_noise_and_old_gaps() {
    let root =
        std::env::temp_dir().join(format!("kah-ack-gap-match-{}", crate::relay_store::nonce()));
    kah_core::run_store::ensure_private_directory(&root).unwrap();
    let gap = json!({"request":"review-one","origin":"report_validation","error":"REVIEW_FOLLOWUP_RISK_MISSING stage=verification"});
    crate::unattended_report::save(&root, "gap.json", &gap).unwrap();
    assert!(host_failure(&root, "review-one").unwrap().is_some());
    assert!(host_failure(&root, "review-other").unwrap().is_none());
    let noise = json!({"request":"review-one","origin":"delivery","error":"REVIEW_SUBMISSION_UNCONFIRMED stage=delivery"});
    crate::unattended_report::save(&root, "gap.json", &noise).unwrap();
    assert!(host_failure(&root, "review-one").unwrap().is_none());
    let host = json!({"request":"review-one","origin":"host","error":"REVIEW_SOURCE_DIGEST stage=source_store"});
    crate::unattended_report::save(&root, "gap.json", &host).unwrap();
    assert!(host_failure(&root, "review-one").unwrap().is_some());
    std::fs::remove_dir_all(root).unwrap();
}
