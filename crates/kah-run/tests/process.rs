use kah_codex::exec_events::Event;
use kah_run::process::{Directive, execute};
use std::{
    path::PathBuf,
    process::Command,
    time::{Duration, Instant},
};
fn log(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!("kah-worker-{}-{name}.log", std::process::id()))
}
#[test]
fn silent_worker_can_be_stopped_without_waiting_for_model_output() {
    let mut command = Command::new("/bin/sh");
    command.args(["-c", "read -r input; sleep 30"]);
    let path = log("stop");
    let started = Instant::now();
    let result = execute(command, "task\n", &path, |_| {
        Ok(if started.elapsed() > Duration::from_millis(100) {
            Directive::Stop("USER_STOP".into())
        } else {
            Directive::Continue
        })
    })
    .unwrap();
    assert_eq!(result.stopped.as_deref(), Some("USER_STOP"));
    assert!(started.elapsed() < Duration::from_secs(5));
    std::fs::remove_file(path).unwrap();
}
#[test]
fn reported_usage_reaches_the_controller_before_completion() {
    let mut command = Command::new("/bin/sh");
    command.args(["-c",r#"read -r input; printf '%s\n' '{"type":"thread.started","thread_id":"fixture"}' '{"type":"turn.started"}' '{"type":"turn.completed","usage":{"input_tokens":9,"output_tokens":3}}'"#]);
    let path = log("usage");
    let mut tokens = 0;
    let result = execute(command, "task\n", &path, |event| {
        if let Some(Event::Usage { input, output, .. }) = event {
            tokens += input + output;
        }
        Ok(Directive::Continue)
    })
    .unwrap();
    assert_eq!(tokens, 12);
    assert!(result.success);
    assert!(result.stopped.is_none());
    std::fs::remove_file(path).unwrap();
}

#[test]
fn cancellation_reaps_a_background_writer_after_its_parent_exits() {
    let pid_path = log("background-pid");
    let mut command = Command::new("/bin/sh");
    command
        .args([
            "-c",
            "read -r input; sleep 30 & printf '%s' \"$!\" > \"$1\"; exit 0",
            "fixture",
        ])
        .arg(&pid_path);
    let path = log("background");
    let started = Instant::now();
    let result = execute(command, "task\n", &path, |_| {
        Ok(if started.elapsed() > Duration::from_millis(100) {
            Directive::Stop("USER_STOP".into())
        } else {
            Directive::Continue
        })
    })
    .unwrap();
    assert_eq!(result.stopped.as_deref(), Some("USER_STOP"));
    assert!(started.elapsed() < Duration::from_secs(5));
    let pid = std::fs::read_to_string(&pid_path).unwrap();
    std::thread::sleep(Duration::from_millis(100));
    let alive = std::fs::read_to_string(format!("/proc/{pid}/stat"))
        .is_ok_and(|s| s.split_once(") ").is_some_and(|(_, s)| !s.starts_with('Z')));
    let _ = Command::new("/bin/kill").args(["-KILL", &pid]).output();
    std::fs::remove_file(path).unwrap();
    std::fs::remove_file(pid_path).unwrap();
    assert!(!alive, "a background writer survived cancellation");
}

#[test]
fn a_fatal_native_event_cannot_be_overridden_by_a_zero_exit_status() {
    let mut command = Command::new("/bin/sh");
    command.args(["-c",r#"read -r input; printf '%s\n' '{"type":"error","message":"fatal"}' '{"type":"turn.completed","usage":{"input_tokens":1,"output_tokens":1}}'"#]);
    let path = log("fatal");
    let result = execute(command, "task\n", &path, |_| Ok(Directive::Continue)).unwrap();
    std::fs::remove_file(path).unwrap();
    assert!(!result.success);
}
