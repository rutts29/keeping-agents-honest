use kah_core::unattended::{Limits, RunControl};
use kah_run::store::{Record, Store};
use serde_json::Value;
use std::{fs, path::PathBuf, process::Command};
struct Fixture(PathBuf);
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
#[test]
fn cli_steer_stop_status_and_logs_use_private_run_state() {
    let root = std::env::temp_dir().join(format!(
        "kah-cli-run-controls-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let _cleanup = Fixture(root.clone());
    let base = root.join("kah/runs");
    let mut control = RunControl::new("Implement sum", Limits::default()).unwrap();
    control.start().unwrap();
    let record = Record {
        native: None,
        project: PathBuf::from("/project"),
        domain: "mine".into(),
        worktree: PathBuf::from("/worktree"),
        model: Some("chosen-model".into()),
        effort: Some("high".into()),
        control,
    };
    drop(Store::create(&base, "run-test", record).unwrap());
    let call = |args: &[&str]| {
        let out = Command::new(env!("CARGO_BIN_EXE_kah"))
            .env("XDG_STATE_HOME", &root)
            .args(args)
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        out.stdout
    };
    let all: Value = serde_json::from_slice(&call(&["status", "--json"])).unwrap();
    assert_eq!(all["runs"][0]["id"], "run-test");
    call(&["steer", "run-test", "Keep the public API"]);
    let status: Value = serde_json::from_slice(&call(&["status", "run-test", "--json"])).unwrap();
    assert_eq!(
        status["control"]["inputs"][1]["text"],
        "Keep the public API"
    );
    assert_eq!(status["model"], "chosen-model");
    fs::write(base.join("run-test/attempt-0001.log"), "A native event\n").unwrap();
    assert_eq!(
        String::from_utf8(call(&["logs", "run-test"])).unwrap(),
        "A native event\n"
    );
    call(&["stop", "run-test"]);
    let status: Value = serde_json::from_slice(&call(&["status", "run-test", "--json"])).unwrap();
    assert_eq!(status["control"]["status"], "stopping");
    fs::remove_dir_all(root).unwrap();
}
