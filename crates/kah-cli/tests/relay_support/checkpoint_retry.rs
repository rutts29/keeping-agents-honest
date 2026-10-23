use super::*;
use kah_cli::observer_review::{failure::Origin, runtime, state::Store as Reviews};

pub(super) fn gap(worker: &str, previous: &mut Option<Value>) {
    let path = Reviews::directory(worker).unwrap().join("gap.json");
    let Ok(bytes) = fs::read(path) else { return };
    let gap: Value = serde_json::from_slice(&bytes).unwrap();
    assert!(
        kah_cli::observer_review::redelivery::gap_retryable(&gap),
        "checkpoint gap: {gap}"
    );
    if previous.as_ref() == Some(&gap) {
        return;
    }
    let state = Reviews::read(worker).unwrap().unwrap();
    let pane = Pane {
        id: state.observer.pane.unwrap(),
    };
    if matches!(
        pane.inspect().unwrap()["agent_status"].as_str(),
        Some("idle" | "done")
    ) {
        retry_outcome(runtime::retry_pending(worker)).unwrap();
        *previous = Some(gap);
    }
}

fn retry_outcome(result: kah_cli::Result<()>) -> kah_cli::Result<()> {
    match result {
        // A delivery acknowledgement gap retains the same pending request.
        // Watch its live native turn/report instead of aborting and killing it.
        Err(error)
            if recoverable(
                kah_cli::observer_review::failure::origin(error.as_ref()),
                &error.to_string(),
            ) =>
        {
            Ok(())
        }
        other => other,
    }
}

fn recoverable(origin: Origin, error: &str) -> bool {
    kah_cli::observer_review::redelivery::retryable(origin, error)
}

#[test]
fn late_native_binding_does_not_abort_a_review_that_is_already_working() {
    // A handshake gap is followed by a bound native session and Working
    // pane. The existing loop must keep watching; redeliver only once idle.
    let error = "REVIEW_SUBMISSION_UNCONFIRMED stage=delivery: cold native binding absent; request retained";
    assert!(recoverable(Origin::Delivery, error));
    assert!(recoverable(
        Origin::Delivery,
        "REVIEW_PREPARATION_MISSING stage=observer: native turn ended without an acknowledged selection; request retained"
    ));
    assert!(!recoverable(
        Origin::ReportValidation,
        "REVIEW_SESSION_MISMATCH stage=hook"
    ));
    assert!(recoverable(
        Origin::ReportValidation,
        "CONTEXT_UNKNOWN_ITEM stage=context_selection"
    ));
}

#[test]
fn a_retained_retry_delivery_gap_does_not_kill_the_active_native_review() {
    assert!(retry_outcome(kah_cli::observer_review::failure::validation(Origin::Delivery, ||Err("REVIEW_SUBMISSION_UNCONFIRMED stage=delivery: native receipt absent; inspect private review/delivery-gap.json".into()))).is_ok());
    assert!(retry_outcome(Err("REVIEW_SESSION_MISMATCH stage=hook".into())).is_err());
}

#[test]
fn fixture_uses_product_retry_policy_for_liveness_and_keeps_identity_errors_fatal() {
    assert!(recoverable(
        Origin::Delivery,
        "REVIEW_OBSERVER_STALLED stage=liveness: no native activity"
    ));
    assert!(!recoverable(
        Origin::ReportValidation,
        "REVIEW_HOOK_SCOPE stage=hook"
    ));
    assert!(!recoverable(
        Origin::ReportValidation,
        "REVIEW_SESSION_MISMATCH stage=hook"
    ));
}

#[test]
fn real_claude_uri_in_source_field_diagnostic_keeps_review_recoverable() {
    let error = "REVIEW_SOURCE_UNKNOWN stage=evidence: citation.source=\"native:claude:00000000-0000-4000-8000-000000000001\" is not a document id. Use manifest document ids, not URIs or paths. Matching URI document ids: [\"transcript\"]";
    assert!(recoverable(Origin::ReportValidation, error));
    assert!(
        retry_outcome(kah_cli::observer_review::failure::validation(
            Origin::ReportValidation,
            || Err(error.into())
        ))
        .is_ok()
    );
    assert!(recoverable(
        Origin::ReportValidation,
        "REVIEW_REQUIREMENT_COVERAGE stage=decision_units: active item goal has no named requirement"
    ));
    for error in [
        "REVIEW_REPORT_JSON stage=report",
        "REVIEW_EVIDENCE_EMPTY stage=evidence",
        "REVIEW_FINDING_EMPTY stage=evidence",
        "REVIEW_QUOTE_MISSING stage=evidence",
        "REVIEW_CHECK_PROVENANCE stage=decision_units",
    ] {
        assert!(recoverable(Origin::ReportValidation, error), "{error}");
    }
    assert!(!recoverable(
        Origin::ReportValidation,
        "REVIEW_SOURCE_DIGEST stage=source_store"
    ));
}
