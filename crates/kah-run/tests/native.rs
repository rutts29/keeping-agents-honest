use kah_core::unattended::{Limits, RunControl};
use kah_run::native::Native;

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
