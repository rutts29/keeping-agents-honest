use super::*;
#[test]
fn delivery_noise_cannot_undo_ack_but_host_errors_are_not_retried_as_success() {
    for error in [
        "REVIEW_REPORT_MISSING stage=observer",
        "REVIEW_OBSERVER_STALLED stage=liveness",
        "REVIEW_SUBMISSION_UNCONFIRMED stage=delivery",
    ] {
        assert!(!correction_required(error));
    }
    for error in [
        "JUDGE_CHOICE stage=judge",
        "JEV_HTTP_STATUS stage=judge",
        "REVIEW_PHASE_CLAIM_UNCERTAIN stage=checkpoint",
        "REVIEW_SOURCE_DIGEST stage=source_store",
    ] {
        assert!(
            correction_required(error),
            "host failure must remain visible, not repeated every sample: {error}"
        );
    }
}
