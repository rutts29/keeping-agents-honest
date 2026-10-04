use super::*;
use kah_cli::observer_review::{runtime, state::Store as Reviews};

pub(super) fn gap(worker: &str, previous: &mut Option<Value>) {
    let path = Reviews::directory(worker).unwrap().join("gap.json");
    let Ok(bytes) = fs::read(path) else { return };
    let gap: Value = serde_json::from_slice(&bytes).unwrap();
    let error = gap["error"].as_str().unwrap();
    assert!(
        error.starts_with("REVIEW_DEFERRAL_")
            || error.starts_with("REVIEW_REPORT_MISSING")
            || error.starts_with("REVIEW_PHASE_CLAIM_UNCERTAIN")
            || error.starts_with("REVIEW_CHECKPOINT_EVIDENCE_MISSING"),
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
        runtime::retry_pending(worker).unwrap();
        *previous = Some(gap);
    }
}
