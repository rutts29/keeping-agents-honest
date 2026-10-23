use kah_cli::observer_review::{state::Purpose, workspace};

#[test]
fn new_checkpoint_and_drift_reports_require_named_decisions() {
    // Steering uses the same named-decision pattern as checkpoint and drift reports.
    for purpose in [Purpose::Checkpoint, Purpose::Drift, Purpose::Steering] {
        let schema = workspace::schema(&purpose);
        assert!(
            schema["required"]
                .as_array()
                .unwrap()
                .iter()
                .any(|x| x == "decision_units")
        );
        let instruction = workspace::decision_instructions(&purpose);
        assert!(instruction.contains("semantic"));
        assert!(instruction.contains("existing ledger"));
        assert!(instruction.contains("needs_user"));
        assert!(instruction.contains("proportion"));
        assert!(!instruction.contains("stronger model"));
    }
}

#[test]
fn new_native_submission_rejects_a_legacy_checkpoint_report() {
    use kah_cli::observer_review::state::Request;
    use kah_observer::review_evidence::Report;
    use serde_json::json;
    let mut request = Request {
        id: "review-new".into(),
        worker: "worker".into(),
        purpose: Purpose::Checkpoint,
        turn: "t".into(),
        head: None,
        intent: json!({}),
        trigger: json!({}),
        documents: vec![],
        created_ms: 0,
    };
    let report: Report =
        serde_json::from_value(json!({"request_id":request.id,"findings":[]})).unwrap();
    assert!(
        workspace::require_units(&request, &report)
            .unwrap_err()
            .to_string()
            .starts_with("REVIEW_UNITS_REQUIRED")
    );
    request.purpose = Purpose::Preparation;
    workspace::require_units(&request, &report).unwrap();
}

#[test]
fn confirmed_v4_drift_corrects_but_uncertainty_does_not_masquerade_as_a_violation() {
    use kah_cli::observer_review::consume::needs_reminder;
    use kah_observer::judge::{Judgment, Metrics};
    use serde_json::json;
    let judgment = Judgment {
        backend: "jev".into(),
        model: "jev-latest".into(),
        answers: Default::default(),
        metrics: Metrics::default(),
        fallback_reason: None,
    };
    assert!(needs_reminder(&json!({"state":"drift_failed"}), &judgment));
    assert!(!needs_reminder(
        &json!({"state":"drift_needs_thinker"}),
        &judgment
    ));
    assert!(!needs_reminder(
        &json!({"state":"drift_needs_user"}),
        &judgment
    ));
}
