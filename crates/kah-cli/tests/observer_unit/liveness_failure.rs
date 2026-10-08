use super::*;

#[test]
fn same_failed_expansion_is_not_renotified_but_new_evidence_or_failed_delivery_is() {
    let key = json!({"class":"failed_host_expansion","request_digest":"a","report_digest":"b"});
    let prior =
        json!({"request":"review-a","notice_state":key,"submitted":true,"cause":"old wording"});
    assert!(already_notified(Some(&prior), "review-a", Some(&key)));
    assert!(!already_notified(Some(&prior), "review-b", Some(&key)));
    let changed = json!({"class":"failed_host_expansion","request_digest":"a","report_digest":"c"});
    assert!(!already_notified(Some(&prior), "review-a", Some(&changed)));
    let mut undelivered = prior;
    undelivered["submitted"] = false.into();
    assert!(!already_notified(
        Some(&undelivered),
        "review-a",
        Some(&key)
    ));
}

#[test]
fn a_failed_sample_is_retained_and_visible_without_requiring_readable_review_state() {
    let worker = format!("watch-error-{}", std::process::id());
    let root = Store::directory(&worker).unwrap();
    kah_core::run_store::ensure_private_directory(&root).unwrap();
    std::fs::write(root.join("state.json"), b"invalid state").unwrap();
    let error = "REVIEW_SOURCE_IO stage=source_store: /missing/block-abc: No such file";
    record(&worker, &error).unwrap();
    let saved: Value = state::read(&root.join("liveness/watch-error.json"))
        .unwrap()
        .unwrap();
    assert_eq!(saved["error"], error);
    assert_eq!(saved["submitted"], false);
    let notice: Value = state::read(&root.join("liveness/notice.json"))
        .unwrap()
        .unwrap();
    assert!(
        notice["text"]
            .as_str()
            .unwrap()
            .contains("/missing/block-abc")
    );
    assert!(root.join("state.json").exists());
    recovered(&worker).unwrap();
    let recovery: Value = state::read(&root.join("liveness/watch-error.json"))
        .unwrap()
        .unwrap();
    assert_eq!(recovery["error"], error);
    assert!(recovery["recovered_ms"].is_u64());
    std::fs::remove_dir_all(root.parent().unwrap()).unwrap();
}
