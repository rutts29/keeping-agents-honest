use kah_core::unattended::{Limits, RunControl, RunStatus};
use kah_run::{
    attempt, commands,
    store::{Record, Store},
};
use std::{
    fs,
    path::PathBuf,
    process::Command,
    thread,
    time::{Duration, SystemTime, UNIX_EPOCH},
};
struct Fixture(PathBuf);
impl Fixture {
    fn new(limits: Limits) -> Self {
        let root = std::env::temp_dir().join(format!(
            "kah-attempt-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let mut control = RunControl::new("task", limits).unwrap();
        control.start().unwrap();
        drop(
            Store::create(
                &root,
                "run-test",
                Record {
                    project: PathBuf::from("/project"),
                    domain: "mine".into(),
                    worktree: PathBuf::from("/worktree"),
                    model: None,
                    effort: None,
                    control,
                },
            )
            .unwrap(),
        );
        Self(root)
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
#[test]
fn steering_interrupts_the_current_attempt_and_remains_user_input() {
    let f = Fixture::new(Limits::default());
    let base = f.0.clone();
    let steering = thread::spawn(move || {
        thread::sleep(Duration::from_millis(150));
        commands::steer(&base, "run-test", "Use BETA").unwrap();
    });
    let mut command = Command::new("/bin/sh");
    command.args(["-c", "read -r input; sleep 30"]);
    let result = attempt::run(
        &f.0,
        "run-test",
        command,
        "task\n",
        &f.0.join("run-test/attempt-1.log"),
    )
    .unwrap();
    steering.join().unwrap();
    assert_eq!(result.stopped.as_deref(), Some("RUN_STEER"));
    let record = commands::view(&f.0, "run-test").unwrap();
    assert_eq!(record.control.inputs()[1].text, "Use BETA");
    assert_eq!(record.control.status(), RunStatus::Running);
}
#[test]
fn usage_guard_stops_after_accounting_reported_native_tokens() {
    let f = Fixture::new(Limits {
        tokens: Some(10),
        seconds: None,
    });
    let mut command = Command::new("/bin/sh");
    command.args(["-c",r#"read -r input; printf '%s\n' '{"type":"thread.started","thread_id":"fixture"}' '{"type":"turn.completed","usage":{"input_tokens":9,"output_tokens":3}}'; sleep 30"#]);
    let result = attempt::run(
        &f.0,
        "run-test",
        command,
        "task\n",
        &f.0.join("run-test/attempt-1.log"),
    )
    .unwrap();
    assert_eq!(result.stopped.as_deref(), Some("RUN_TOKEN_BUDGET"));
    let record = commands::view(&f.0, "run-test").unwrap();
    assert_eq!(record.control.tokens_used(), 12);
    assert_eq!(record.control.status(), RunStatus::BudgetStopped);
}

#[test]
fn malformed_native_output_stops_the_run_and_preserves_elapsed_time() {
    let f = Fixture::new(Limits::default());
    let mut command = Command::new("/bin/sh");
    command.args(["-c", "read -r input; sleep 0.1; printf 'invalid-json\n'"]);
    let result = attempt::run(
        &f.0,
        "run-test",
        command,
        "task\n",
        &f.0.join("run-test/attempt-1.log"),
    );
    assert!(result.err().unwrap().starts_with("CODEX_EXEC_JSON"));
    let record = commands::view(&f.0, "run-test").unwrap();
    assert_eq!(record.control.status(), RunStatus::Stopped);
    assert!(record.control.elapsed_ms() >= 100);
}
