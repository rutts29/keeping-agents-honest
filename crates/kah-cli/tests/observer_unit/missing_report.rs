use super::*;

#[test]
fn a_missing_report_is_retried_once_then_retired_never_left_pending() {
    // If the observer's executor transport drops and its turn ends without
    // a report, the queued request must not remain pending indefinitely.
    let directory = std::env::temp_dir().join(format!(
        "kah-missing-report-{}",
        crate::relay_store::nonce()
    ));
    kah_core::run_store::ensure_private_directory(&directory).unwrap();
    assert_eq!(next(&directory, "review-1").unwrap(), Next::Retry);
    assert_eq!(next(&directory, "review-1").unwrap(), Next::Retire);
    // Each request gets its own single retry.
    assert_eq!(next(&directory, "review-2").unwrap(), Next::Retry);
    std::fs::remove_dir_all(directory).unwrap();
}

#[test]
fn a_retired_request_is_recorded_as_not_reviewed_and_never_as_a_pass() {
    let result = not_reviewed("review-1", &json!("abc"));
    assert_eq!(result["status"], "not_reviewed");
    assert_eq!(result["assessment"]["passed"], Value::Null);
    assert_eq!(result["gap"]["code"], "REVIEW_OBSERVER_NO_REPORT");
    assert!(NO_REPORT.starts_with("REVIEW_OBSERVER_NO_REPORT stage=observer"));
}
