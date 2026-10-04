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
        runtime::retry_pending(worker).unwrap();
        *previous = Some(gap);
    }
}

fn recoverable(error: &str) -> bool {
    [
        "REVIEW_DEFERRAL_",
        "REVIEW_REPORT_MISSING",
        "REVIEW_PREPARATION_MISSING",
        "REVIEW_PHASE_CLAIM_UNCERTAIN",
        "REVIEW_CHECKPOINT_SCOPE_MISSING",
        "REVIEW_CHECKPOINT_EVIDENCE_MISSING",
        "REVIEW_SUBMISSION_UNCONFIRMED",
    ]
    .iter()
    .any(|code| error.starts_with(code))
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
}
