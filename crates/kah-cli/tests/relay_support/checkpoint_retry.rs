use super::*;
use kah_cli::observer_review::{runtime, state::Store as Reviews};

pub(super) fn gap(worker: &str, previous: &mut Option<Value>) {
    let path = Reviews::directory(worker).unwrap().join("gap.json");
    let Ok(bytes) = fs::read(path) else { return };
    let gap: Value = serde_json::from_slice(&bytes).unwrap();
    let error = gap["error"].as_str().unwrap();
    assert!(recoverable(error), "checkpoint gap: {gap}");
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
        Err(error) if recoverable(&error.to_string()) => Ok(()),
        other => other,
    }
}

fn recoverable(error: &str) -> bool {
    kah_cli::observer_review::redelivery::retryable(error)
}

#[test]
fn late_native_binding_does_not_abort_a_review_that_is_already_working() {
    // Real v2 journey: 15s handshake gap, then bound native session and Working
    // pane. The existing loop must keep watching; redeliver only once idle.
    let error = "REVIEW_SUBMISSION_UNCONFIRMED stage=delivery: cold native binding absent; request retained";
    assert!(recoverable(error));
    assert!(recoverable(
        "REVIEW_PREPARATION_MISSING stage=observer: native turn ended without an acknowledged selection; request retained"
    ));
    assert!(!recoverable("REVIEW_SESSION_MISMATCH stage=hook"));
    assert!(recoverable("CONTEXT_UNKNOWN_ITEM stage=context_selection"));
}

#[test]
fn a_retained_retry_delivery_gap_does_not_kill_the_active_native_review() {
    assert!(retry_outcome(Err("REVIEW_SUBMISSION_UNCONFIRMED stage=delivery: native receipt absent; inspect private review/delivery-gap.json".into())).is_ok());
    assert!(retry_outcome(Err("REVIEW_SESSION_MISMATCH stage=hook".into())).is_err());
}

#[test]
fn fixture_uses_product_retry_policy_for_liveness_and_keeps_identity_errors_fatal() {
    assert!(recoverable(
        "REVIEW_OBSERVER_STALLED stage=liveness: no native activity"
    ));
    assert!(!recoverable("REVIEW_HOOK_SCOPE stage=hook"));
    assert!(!recoverable("REVIEW_SESSION_MISMATCH stage=hook"));
}

#[test]
fn real_claude_uri_in_source_field_diagnostic_keeps_review_recoverable() {
    let error = "REVIEW_SOURCE_UNKNOWN stage=evidence: citation.source=\"native:claude:00000000-0000-4000-8000-01a11278cba5\" is not a document id. Use manifest document ids, not URIs or paths. Matching URI document ids: [\"transcript\"]";
    assert!(recoverable(error));
    assert!(retry_outcome(Err(error.into())).is_ok());
    assert!(recoverable(
        "REVIEW_REQUIREMENT_COVERAGE stage=decision_units: active item goal has no named requirement"
    ));
    for error in [
        "REVIEW_REPORT_JSON stage=report",
        "REVIEW_EVIDENCE_EMPTY stage=evidence",
        "REVIEW_FINDING_EMPTY stage=evidence",
        "REVIEW_QUOTE_MISSING stage=evidence",
        "REVIEW_CHECK_PROVENANCE stage=decision_units",
    ] {
        assert!(recoverable(error), "{error}");
    }
    assert!(!recoverable("REVIEW_SOURCE_DIGEST stage=source_store"));
}
