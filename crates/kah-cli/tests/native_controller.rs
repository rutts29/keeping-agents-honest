use std::{fs, process::Command};

#[test]
fn startup_failure_stops_the_run_and_leaves_a_report() {
    let root = std::env::temp_dir().join(format!("kah-native-startup-{}", std::process::id()));
    fs::create_dir(&root).unwrap();
    let output = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "startup_failure_child", "--nocapture"])
        .env("KAH_STARTUP_FAILURE_FIXTURE", &root)
        .env("HOME", &root)
        .env_remove("XDG_STATE_HOME")
        .output()
        .unwrap();
    fs::remove_dir_all(root).unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stdout)
    );
}

#[test]
fn startup_failure_child() {
    let Some(home) = std::env::var_os("KAH_STARTUP_FAILURE_FIXTURE") else {
        return;
    };
    let home = std::path::PathBuf::from(home);
    let base = kah_core::run_store::RunStore::default_state_root()
        .unwrap()
        .join("runs");
    let record = kah_run::store::Record {
        project: home.join("source"),
        domain: "mine".into(),
        worktree: home.join("worktree"),
        model: None,
        effort: None,
        native: Some(kah_run::native::Native::new("codex").unwrap()),
        control: kah_core::unattended::RunControl::new("fixture", Default::default()).unwrap(),
    };
    drop(kah_run::store::Store::create(&base, "run-startup-failure", record).unwrap());
    assert!(kah_cli::native_controller::execute(&home, "run-startup-failure", false).is_err());
    let record = kah_run::store::Store::read(&base, "run-startup-failure").unwrap();
    assert_eq!(
        record.control.status(),
        kah_core::unattended::RunStatus::Stopped
    );
    let report: serde_json::Value =
        serde_json::from_slice(&fs::read(base.join("run-startup-failure/report.json")).unwrap())
            .unwrap();
    assert!(
        report["error"]
            .as_str()
            .unwrap()
            .contains("RUN_SETTINGS_IO")
    );
}
