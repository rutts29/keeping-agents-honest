use serde_json::json;
#[test]
fn a_checkpoint_cannot_judge_user_intent_without_any_selected_intent_evidence() {
    let missing = [
        json!({"evidence":{"topic":"integration"}}),
        json!({"evidence":{"topic":"drift"}}),
    ];
    let error = kah_cli::observer_review::checkpoint::require_coverage(&missing)
        .unwrap_err()
        .to_string();
    assert!(error.starts_with("REVIEW_CHECKPOINT_EVIDENCE_MISSING"));
    assert!(error.contains("intent"));
    let complete = [
        json!({"evidence":{"topic":"intent"}}),
        json!({"evidence":{"topic":"integration"}}),
        json!({"evidence":{"topic":"drift"}}),
    ];
    kah_cli::observer_review::checkpoint::require_coverage(&complete).unwrap();
}
