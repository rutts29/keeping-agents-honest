use serde_json::json;
#[test]
fn a_checkpoint_cannot_judge_user_intent_without_any_selected_intent_evidence() {
    let missing = [
        json!({"evidence":{"topic":"integration","checkpoint_scope":{"phase":"Phase 2","components":["parser","renderer"],"authorization_citation":0}}}),
        json!({"evidence":{"topic":"drift"}}),
    ];
    let error = kah_cli::observer_review::checkpoint::require_coverage(&missing)
        .unwrap_err()
        .to_string();
    assert!(error.starts_with("REVIEW_CHECKPOINT_EVIDENCE_MISSING"));
    assert!(error.contains("intent"));
    let complete = [
        json!({"evidence":{"topic":"intent"}}),
        json!({"evidence":{"topic":"integration","checkpoint_scope":{"phase":"Phase 2","components":["parser","renderer"],"authorization_citation":0}}}),
        json!({"evidence":{"topic":"drift"}}),
    ];
    kah_cli::observer_review::checkpoint::require_coverage(&complete).unwrap();
    let mut no_scope = complete;
    no_scope[1]["evidence"]
        .as_object_mut()
        .unwrap()
        .remove("checkpoint_scope");
    assert!(
        kah_cli::observer_review::checkpoint::require_coverage(&no_scope)
            .unwrap_err()
            .to_string()
            .starts_with("REVIEW_CHECKPOINT_SCOPE_MISSING")
    );
}
