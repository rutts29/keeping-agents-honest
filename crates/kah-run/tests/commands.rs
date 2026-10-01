use kah_core::unattended::{Limits, RunControl, RunStatus};
use kah_run::{
    commands,
    store::{Record, Store},
};
use std::{fs, path::PathBuf};

#[test]
fn operator_controls_persist_steering_and_stop_without_touching_worktree() {
    let root = std::env::temp_dir().join(format!("kah-run-commands-{}", std::process::id()));
    let mut control = RunControl::new("Implement sum", Limits::default()).unwrap();
    control.start().unwrap();
    let record = Record {
        native: None,
        project: PathBuf::from("/project"),
        domain: "mine".into(),
        worktree: PathBuf::from("/worktree"),
        model: None,
        effort: None,
        control,
    };
    drop(Store::create(&root, "run-controls", record).unwrap());
    assert_eq!(
        commands::steer(&root, "run-controls", "Preserve the API").unwrap(),
        1
    );
    assert_eq!(
        commands::view(&root, "run-controls")
            .unwrap()
            .control
            .inputs()
            .len(),
        2
    );
    commands::stop(&root, "run-controls").unwrap();
    assert_eq!(
        commands::view(&root, "run-controls")
            .unwrap()
            .control
            .status(),
        RunStatus::Stopping
    );
    assert!(commands::view(&root, "run-missing").is_err());
    assert!(!root.join("run-missing").exists());
    fs::remove_dir_all(root).unwrap();
}
#[test]
fn logs_never_follow_a_symlink() {
    use std::os::unix::fs::symlink;
    let root = std::env::temp_dir().join(format!("kah-run-logs-{}", std::process::id()));
    let record = Record {
        native: None,
        project: PathBuf::from("/project"),
        domain: "mine".into(),
        worktree: PathBuf::from("/worktree"),
        model: None,
        effort: None,
        control: RunControl::new("task", Limits::default()).unwrap(),
    };
    drop(Store::create(&root, "run-logs", record).unwrap());
    fs::write(root.join("private"), "not a log").unwrap();
    symlink(root.join("private"), root.join("run-logs/attempt-0001.log")).unwrap();
    assert!(commands::logs(&root, "run-logs").is_err());
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn native_runs_queue_operator_steering_for_native_delivery() {
    let root = std::env::temp_dir().join(format!("kah-native-controls-{}", std::process::id()));
    let record = Record {
        native: Some(kah_run::native::Native::new("codex").unwrap()),
        project: PathBuf::from("/project"),
        domain: "mine".into(),
        worktree: PathBuf::from("/worktree"),
        model: None,
        effort: None,
        control: RunControl::new("task", Limits::default()).unwrap(),
    };
    drop(Store::create(&root, "run-native", record).unwrap());
    let sequence = commands::steer(&root, "run-native", "steer: exact words").unwrap();
    let record = Store::read(&root, "run-native").unwrap();
    assert_eq!(record.control.inputs()[1].text, "steer: exact words");
    assert_eq!(record.native.unwrap().pending.front(), Some(&sequence));
    fs::remove_dir_all(root).unwrap();
}
