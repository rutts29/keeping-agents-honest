use kah_core::unattended::{CompletionEvidence, Limits, RunControl, RunStatus};

#[test]
fn steering_is_ordered_and_invalidates_an_old_completion_verdict() {
    let mut run = RunControl::new("Implement sum", Limits::default()).unwrap();
    run.start().unwrap();
    run.checkpoint("a".repeat(40)).unwrap();
    let proof = CompletionEvidence {
        ledger_user_sequence: 0,
        commit: "a".repeat(40),
        tests_passed: true,
        outcomes_met: true,
    };
    assert_eq!(run.task(), "Implement sum");
    assert_eq!(run.steer("Also support negative values").unwrap(), 1);
    assert!(!run.complete(&proof).unwrap());
    assert_eq!(run.status(), RunStatus::Running);
    let mut fresh = proof;
    fresh.ledger_user_sequence = 1;
    assert!(run.complete(&fresh).unwrap());
}

#[test]
fn resume_keeps_budget_usage_and_requires_remaining_allowance() {
    let mut run = RunControl::new(
        "task",
        Limits {
            tokens: Some(100),
            seconds: Some(60),
        },
    )
    .unwrap();
    run.start().unwrap();
    run.account(40, 10, 1_000);
    run.stop();
    run.resume().unwrap();
    assert_eq!(run.tokens_used(), 50);
    run.account(40, 10, 1_000);
    assert_eq!(run.status(), RunStatus::BudgetStopped);
    assert!(run.resume().is_err());
    assert_eq!(run.tokens_used(), 100);
}

#[test]
fn tests_and_judgment_must_cover_the_current_checkpoint() {
    let mut run = RunControl::new("task", Limits::default()).unwrap();
    run.start().unwrap();
    run.checkpoint("b".repeat(40)).unwrap();
    let mut proof = CompletionEvidence {
        ledger_user_sequence: 0,
        commit: "a".repeat(40),
        tests_passed: true,
        outcomes_met: true,
    };
    assert!(!run.complete(&proof).unwrap());
    proof.commit = "b".repeat(40);
    proof.tests_passed = false;
    assert!(!run.complete(&proof).unwrap());
    proof.tests_passed = true;
    proof.outcomes_met = false;
    assert!(!run.complete(&proof).unwrap());
}

#[test]
fn time_and_external_action_stops_never_report_completion() {
    let mut run = RunControl::new(
        "task",
        Limits {
            tokens: None,
            seconds: Some(2),
        },
    )
    .unwrap();
    run.start().unwrap();
    run.account(0, 0, 2_000);
    assert_eq!(run.status(), RunStatus::TimeStopped);
    let mut other = RunControl::new("task", Limits::default()).unwrap();
    other.start().unwrap();
    other.block_external("Push needs user promotion").unwrap();
    assert_eq!(other.status(), RunStatus::ExternalActionRequired);
}

#[test]
fn invalid_or_reordered_persisted_user_input_is_rejected() {
    let run = RunControl::new("task", Limits::default()).unwrap();
    let mut value = serde_json::to_value(&run).unwrap();
    value["inputs"] = serde_json::json!([]);
    assert!(serde_json::from_value::<RunControl>(value.clone()).is_err());
    value["inputs"] = serde_json::json!([{"sequence":1,"text":"injected out of order"}]);
    assert!(serde_json::from_value::<RunControl>(value).is_err());
}

#[test]
fn stop_before_start_prevents_a_queued_run_from_launching() {
    let mut run = RunControl::new("task", Limits::default()).unwrap();
    run.stop();
    assert_eq!(run.status(), RunStatus::Stopped);
    assert!(run.start().is_err());
}
