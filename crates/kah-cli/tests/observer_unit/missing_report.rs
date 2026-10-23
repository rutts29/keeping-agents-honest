use super::*;

#[test]
fn a_missing_report_is_retried_once_then_retired_never_left_pending() {
    // The observer transport dropped and its turn ended without a report.
    // The request must retry or retire instead of remaining pending.
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
    for (gap, code) in [
        (NO_REPORT, "REVIEW_OBSERVER_NO_REPORT"),
        (UNDELIVERED, "REVIEW_OBSERVER_UNDELIVERED"),
    ] {
        let result = not_reviewed("review-1", &json!("abc"), &gap);
        assert_eq!(result["status"], "not_reviewed");
        assert_eq!(result["assessment"]["passed"], Value::Null);
        assert_eq!(result["gap"]["code"], code);
        assert!(gap.error().starts_with(&format!("{code} stage=observer: ")));
        assert!(gap.error().ends_with("this request was NOT reviewed"));
    }
}

#[test]
fn an_unaccepted_request_is_retired_only_after_the_delivery_deadline() {
    // A checkpoint request waited without a native receipt.
    let first = 1_000_000;
    assert!(!undelivered_expired(
        Some(first),
        first + UNDELIVERED_MS - 1,
        false
    ));
    assert!(undelivered_expired(
        Some(first),
        first + UNDELIVERED_MS,
        false
    ));
    // A native receipt means delivery happened; the missing-report path owns it.
    assert!(!undelivered_expired(Some(first), first + 23 * 60_000, true));
    // Queued 12 min behind a live native turn and never redelivered yet:
    // it is attempted, never retired.
    assert!(!undelivered_expired(None, first + 12 * 60_000, false));
}

#[test]
fn the_delivery_deadline_starts_at_the_first_host_redelivery() {
    let directory =
        std::env::temp_dir().join(format!("kah-undelivered-{}", crate::relay_store::nonce()));
    kah_core::run_store::ensure_private_directory(&directory).unwrap();
    assert_eq!(first_attempt_ms(&directory, "review-1").unwrap(), None);
    crate::unattended_report::save(
        &directory,
        "redelivery.json",
        &json!({"requests":{"review-1":{"attempts":[{"number":1,"at_ms":10},{"number":2,"at_ms":20}]}}}),
    )
    .unwrap();
    assert_eq!(first_attempt_ms(&directory, "review-1").unwrap(), Some(10));
    assert_eq!(first_attempt_ms(&directory, "review-2").unwrap(), None);
    std::fs::remove_dir_all(directory).unwrap();
}
