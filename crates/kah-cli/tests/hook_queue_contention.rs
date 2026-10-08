//! Hook JSON fields: openai/codex rust-v0.160.0 core hooks implementation;
//! Native worker hooks must never wait on an intentionally held admission lock.
//! The 2s deadlock-test safety guard releases that lock and waits for cleanup;
//! it is not a product timeout or a deadline on progressing observer work.
use kah_observer::session::Binding;
use serde_json::json;
use sha2::{Digest, Sha256};
use std::os::unix::fs::OpenOptionsExt;
use std::{
    fs,
    io::Write,
    path::PathBuf,
    process::{Command, Stdio},
    time::{Duration, Instant},
};
struct Fixture {
    home: PathBuf,
    project: PathBuf,
    root: PathBuf,
    binding: Binding,
}
impl Fixture {
    fn new() -> Self {
        let home =
            std::env::temp_dir().join(format!("kah-hook-queue-{}", kah_cli::relay_store::nonce()));
        let project = home.join("repos/mine/project");
        fs::create_dir_all(&project).unwrap();
        let base = home.join("state/kah");
        let root = kah_observer::session::Store::directory(&base, "hook-fixture").join("review");
        kah_core::run_store::ensure_private_directory(root.parent().unwrap()).unwrap();
        for directory in [root.join("queue-receipts"), home.join(".local/bin")] {
            kah_core::run_store::ensure_private_directory(&directory).unwrap();
        }
        fs::copy(env!("CARGO_BIN_EXE_kah"), home.join(".local/bin/kah")).unwrap();
        let policies = base.join("session-policies");
        kah_core::run_store::ensure_private_directory(&policies).unwrap();
        let policy = kah_cli::session_policy::SessionPolicy {
            project: project.clone(),
            policy: "mine-yolo".into(),
            overlay: home.join(".codex"),
        };
        fs::write(
            policies.join(format!("{:x}.json", Sha256::digest(b"hook-fixture"))),
            serde_json::to_vec(&policy).unwrap(),
        )
        .unwrap();
        let binding = Binding {
            session: "hook-fixture".into(),
            project: project.clone(),
            codex_home: home.join(".codex"),
            native: "/bin/true".into(),
            model: "gpt-6.1-sol".into(),
            effort: "low".into(),
            timeout_secs: 2,
            judge: Default::default(),
            jev_model: "jev-latest".into(),
        };
        Self {
            home,
            project,
            root,
            binding,
        }
    }
    fn command(&self) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_kah"));
        command
            .arg("__hook")
            .env_clear()
            .env("HOME", &self.home)
            .env("XDG_STATE_HOME", self.home.join("state"))
            .env("PATH", std::env::var_os("PATH").unwrap())
            .env(
                "KAH_OBSERVER_BINDING",
                serde_json::to_string(&self.binding).unwrap(),
            )
            .stdin(Stdio::piped())
            .stdout(Stdio::piped());
        command.env(
            "KAH_SESSION_POLICY",
            serde_json::to_string(&kah_cli::session_policy::SessionPolicy {
                project: self.project.clone(),
                policy: "mine-yolo".into(),
                overlay: self.home.join(".codex"),
            })
            .unwrap(),
        );
        command
    }
}
#[test]
fn native_pre_tool_hook_returns_while_admission_mutex_is_held() {
    let fixture = Fixture::new();
    let lock = fs::OpenOptions::new()
        .create(true)
        .mode(0o600)
        .truncate(false)
        .read(true)
        .write(true)
        .open(fixture.root.join("queue-receipts/delivery-lock"))
        .unwrap();
    lock.lock().unwrap();
    let mut child = fixture.command().spawn().unwrap();
    let started = Instant::now();
    child.stdin.take().unwrap().write_all(json!({"session_id":"hook-fixture","cwd":fixture.project,"hook_event_name":"PreToolUse","turn_id":"turn"}).to_string().as_bytes()).unwrap();
    let mut finished = false;
    while started.elapsed() < Duration::from_secs(2) {
        if child.try_wait().unwrap().is_some() {
            finished = true;
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    drop(lock);
    let output = child.wait_with_output().unwrap();
    let response: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert!(
        fixture.root.join("config.json").exists(),
        "hook must reach enabled review registration: {response}"
    );
    if let Ok(job) = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(fixture.root.join("job-__review-kick.lock"))
    {
        job.lock().unwrap();
    }
    fs::remove_dir_all(&fixture.home).unwrap();
    assert!(
        finished,
        "native hook waited on admission: {:?}, output {response}",
        started.elapsed()
    );
    assert!(
        response.get("systemMessage").is_none()
            || response["systemMessage"]
                .as_str()
                .is_some_and(|text| text.starts_with("kah observer idle: intent ledger is empty")),
        "{response}"
    );
}
