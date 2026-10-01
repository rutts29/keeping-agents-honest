use kah_core::unattended::{Limits, RunControl};
use kah_run::native::Native;

#[test]
fn run_names_are_unique_labels_and_never_contain_terminal_controls() {
    let a = kah_run::native::session_name("run-111-7", "Fix TLS\n retries; $(bad)\u{1b}");
    assert_eq!(a, "kah-run-111-7-fix-tls-retries-bad");
    assert_ne!(
        a,
        kah_run::native::session_name("run-112-7", "Fix TLS retries; $(bad)")
    );
}

#[test]
fn a_terminal_decision_is_not_published_before_its_report() {
    let mut control = RunControl::new("task", Limits::default()).unwrap();
    control.start().unwrap();
    control.stop();
    let mut record = kah_run::store::Record {
        project: "/project".into(),
        domain: "mine".into(),
        worktree: "/worktree".into(),
        model: None,
        effort: None,
        control,
        native: Some(Native::new("claude").unwrap()),
    };
    assert_eq!(
        record.published_status(),
        kah_core::unattended::RunStatus::Stopping
    );
    record.native.as_mut().unwrap().report_ready = Some(true);
    assert_eq!(
        record.published_status(),
        kah_core::unattended::RunStatus::Stopped
    );
}

#[test]
fn resume_continuation_is_delegate_text_and_waits_for_queued_user_input() {
    let mut control = RunControl::new("task", Limits::default()).unwrap();
    let mut native = Native::new("claude").unwrap();
    native.session = Some("saved".into());
    assert!(native.continuation().is_some());
    native.sources(&mut control, &["task".into()]).unwrap();
    assert!(native.continuation().unwrap().starts_with("[kah-builder]"));
    native.pending.push_back(control.steer("new task").unwrap());
    assert!(native.continuation().is_none());
}

#[test]
fn steering_can_queue_during_the_first_turn_before_its_transcript_is_durable() {
    let mut control = RunControl::new("task", Limits::default()).unwrap();
    let mut native = Native::new("codex").unwrap();
    native
        .pending
        .push_back(control.steer("correction").unwrap());
    assert!(!native.can_deliver(false));
    assert!(native.can_deliver(true));
    native.begin_delivery(&control).unwrap();
    native.delivered().unwrap();
    native
        .pending
        .push_back(control.steer("next correction").unwrap());
    assert!(!native.can_deliver(true));
    native
        .sources(&mut control, &["task".into(), "correction".into()])
        .unwrap();
    assert!(native.can_deliver(true));
}

#[test]
fn a_new_user_input_invalidates_old_completion_even_before_native_delivery() {
    let mut control = RunControl::new("task", Limits::default()).unwrap();
    let mut native = Native::new("codex").unwrap();
    native.sources(&mut control, &["task".into()]).unwrap();
    assert!(native.current(&control, 0));
    control.steer("steer: revised task").unwrap();
    assert!(!native.current(&control, 0));
    native.pending.push_back(1);
    assert!(!native.current(&control, 1));
    native.begin_delivery(&control).unwrap();
    native.delivered().unwrap();
    assert!(!native.current(&control, 1));
    native
        .sources(&mut control, &["task".into(), "steer: revised task".into()])
        .unwrap();
    assert!(native.current(&control, 1));
}

#[test]
fn queued_steering_and_direct_native_messages_are_recorded_once() {
    let mut control = RunControl::new("task", Limits::default()).unwrap();
    let mut native = Native::new("codex").unwrap();
    let seq = control.steer("steer: blue").unwrap();
    native.pending.push_back(seq);
    native.sources(&mut control, &["task".into()]).unwrap();
    assert_eq!(
        native.begin_delivery(&control).unwrap().as_deref(),
        Some("steer: blue")
    );
    native.delivered().unwrap();
    native
        .sources(
            &mut control,
            &[
                "task".into(),
                "aside: direct pane message".into(),
                "steer: blue".into(),
            ],
        )
        .unwrap();
    assert_eq!(control.inputs().len(), 3);
    assert!(native.synchronized());
    native
        .sources(
            &mut control,
            &[
                "task".into(),
                "aside: direct pane message".into(),
                "steer: blue".into(),
            ],
        )
        .unwrap();
    assert_eq!(control.inputs().len(), 3);
}

#[test]
fn durable_native_input_recovers_a_delivery_interrupted_before_acknowledgment() {
    let mut control = RunControl::new("task", Limits::default()).unwrap();
    let mut native = Native::new("claude").unwrap();
    native.sources(&mut control, &["task".into()]).unwrap();
    native
        .pending
        .push_back(control.steer("correction").unwrap());
    native.begin_delivery(&control).unwrap();
    let encoded = serde_json::to_string(&native).unwrap();
    let mut resumed: Native = serde_json::from_str(&encoded).unwrap();
    resumed
        .sources(&mut control, &["task".into(), "correction".into()])
        .unwrap();
    assert!(resumed.synchronized());
    assert_eq!(control.inputs().len(), 2);
}

#[test]
fn cumulative_usage_does_not_charge_twice_and_survives_resume() {
    let mut control = RunControl::new(
        "task",
        Limits {
            tokens: Some(100),
            seconds: None,
        },
    )
    .unwrap();
    control.start().unwrap();
    let mut native = Native::new("codex").unwrap();
    native.account(&mut control, 30, 10, 50);
    native.account(&mut control, 30, 10, 50);
    native.account(&mut control, 0, 0, 0);
    native.account(&mut control, 30, 10, 0);
    assert_eq!(control.tokens_used(), 40);
    control.stop();
    control.resume().unwrap();
    native.account(&mut control, 95, 10, 50);
    assert_eq!(control.tokens_used(), 105);
    assert_eq!(
        control.status(),
        kah_core::unattended::RunStatus::BudgetStopped
    );
}
