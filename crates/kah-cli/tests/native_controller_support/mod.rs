use kah_core::{run_store::RunStore, unattended::RunStatus};
use kah_run::store::{Record, Store};
use kah_sandbox::{Domain, Project};
use serde_json::{Value, json};
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
    time::{Duration, Instant},
};
mod capture;
mod cleanup;
mod proof;
pub use cleanup::cleanup;
mod verification;
pub use verification::verification_uses_imported_commit;

pub fn run(root: &Path, agent: &str) -> Value {
    let home = PathBuf::from(std::env::var_os("HOME").unwrap());
    let before = [
        home.join(".claude/settings.json"),
        home.join(".codex/config.toml"),
    ]
    .map(|p| (p.clone(), fs::read(p).unwrap()));
    let name = format!("kah-native-run-{agent}-{}", std::process::id());
    let project = Project::select(&home.join("repos"), &home, Some((Domain::Mine, &name))).unwrap();
    assert!(project.needs_creation);
    project.initialize().unwrap();
    let model = if agent == "claude" {
        "haiku"
    } else {
        "gpt-6-sol"
    };
    let (file_tool, shell_tool) = if agent == "claude" {
        ("Write", "Bash")
    } else {
        ("apply_patch", "exec_command")
    };
    let task = format!(
        "In this disposable worktree, use {file_tool} to implement double.js: define double(n) returning n * 2 and export that function directly with module.exports = double. Use {shell_tool} to check it with Node, then report the result. The kah controller may create checkpoints and run independent verification. Do not push or deploy."
    );
    let output = launch(&home, &project.root, agent, model, &task);
    let id = output
        .lines()
        .find_map(|line| line.strip_prefix("Created "))
        .unwrap();
    fs::write(
        root.join("cleanup.json"),
        json!({"run":id,"source":project.root,"agent":agent}).to_string(),
    )
    .unwrap();
    let base = RunStore::default_state_root().unwrap().join("runs");
    let record = wait(&base, id, agent);
    let report = diagnostics(&Store::path(&base, id).unwrap(), &record.worktree);
    capture::capture(&home, &Store::path(&base, id).unwrap(), &record, &report);
    assert_eq!(
        record.control.status(),
        RunStatus::Completed,
        "{}",
        serde_json::to_string(&record).unwrap()
    );
    assert_eq!(record.control.task(), task);
    assert!(record.control.tokens_used() > 0);
    let native = record.native.as_ref().unwrap();
    assert!(native.native_tokens > 0 && native.observer_tokens > 0);
    assert!(native.synchronized());
    assert!(!project.root.join("double.js").exists());
    proof::native_tools(&home, &Store::path(&base, id).unwrap(), &record);
    assert_eq!(report["tests"]["success"], true);
    for (path, bytes) in before {
        assert_eq!(bytes, fs::read(path).unwrap());
    }
    let version = Command::new(home.join(".local/bin").join(agent))
        .arg("--version")
        .output()
        .unwrap();
    json!({"agent":agent,"version":String::from_utf8(version.stdout).unwrap().trim(),"fixtureModel":model,"fixtureEffort":"low","sourceDomain":"mine","fullNativeSession":true,"ownHerdrPane":true,"verbatimTask":true,"nativeFileTool":true,"nativeShellTool":true,"nativeTokens":native.native_tokens,"observerTokens":native.observer_tokens,"status":"completed","commit":record.control.last_checkpoint(),"tests":report["tests"],"sourceCheckoutUnchanged":true,"globalConfigUnchanged":true,"passed":true})
}
fn launch(home: &Path, project: &Path, agent: &str, model: &str, task: &str) -> String {
    let result = Command::new(home.join(".local/bin/kah"))
        .args([
            "run",
            "--agent",
            agent,
            "--model",
            model,
            "--effort",
            "low",
            "--test",
            proof::VERIFY,
            task,
        ])
        .current_dir(project)
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    String::from_utf8(result.stdout).unwrap()
}
fn wait(base: &Path, id: &str, agent: &str) -> Record {
    let deadline = Instant::now() + Duration::from_secs(250);
    let mut accepted = false;
    loop {
        let record = Store::read(base, id).unwrap();
        if let Some(pane) = record.native.as_ref().unwrap().pane.as_ref() {
            let output = Command::new("herdr")
                .args([
                    "pane", "read", pane, "--source", "recent", "--format", "text",
                ])
                .output()
                .unwrap();
            let screen = String::from_utf8_lossy(&output.stdout);
            if !accepted
                && (screen.contains("Open restricted")
                    || screen.contains("Yes, I trust this folder"))
            {
                std::thread::sleep(Duration::from_millis(400));
                let keys = if agent == "claude" {
                    vec!["Down", "Enter"]
                } else {
                    vec!["Enter"]
                };
                for key in keys {
                    send_key(pane, key);
                }
                accepted = true;
            }
            if !matches!(
                record.control.status(),
                RunStatus::Created | RunStatus::Running | RunStatus::Stopping
            ) && Store::path(base, id).unwrap().join("report.json").exists()
            {
                return record;
            }
            assert!(
                Instant::now() < deadline,
                "run did not finish: {screen}; {}",
                serde_json::to_string(&record).unwrap()
            );
        }
        assert!(
            Instant::now() < deadline,
            "controller did not register its pane"
        );
        std::thread::sleep(Duration::from_millis(300));
    }
}
fn send_key(pane: &str, key: &str) {
    let output = Command::new("herdr")
        .args([
            "pane", "read", pane, "--source", "recent", "--format", "text",
        ])
        .output()
        .unwrap();
    crate::pty::input::send(&output.stdout, "", &mut Vec::new()).unwrap();
    assert!(
        Command::new("herdr")
            .args(["pane", "send-keys", pane, key])
            .output()
            .unwrap()
            .status
            .success()
    );
    std::thread::sleep(Duration::from_millis(350));
}
fn diagnostics(directory: &Path, worktree: &Path) -> Value {
    for path in [
        worktree.to_owned(),
        worktree.parent().unwrap().join("candidate/worktree"),
    ] {
        eprintln!(
            "checkout {}: {:?}",
            path.display(),
            fs::read_dir(&path)
                .map(|entries| entries.flatten().map(|e| e.file_name()).collect::<Vec<_>>())
                .unwrap_or_default()
        );
    }
    let report: Value =
        serde_json::from_slice(&fs::read(directory.join("report.json")).unwrap()).unwrap();
    eprintln!(
        "controller result: {}",
        json!({"status":report["result"],"error":report["error"],"tests":report["tests"]})
    );
    for entry in fs::read_dir(directory).unwrap().flatten() {
        if entry
            .file_name()
            .to_string_lossy()
            .starts_with("completion-")
        {
            eprintln!(
                "completion receipt: {}",
                fs::read_to_string(entry.path()).unwrap()
            );
        }
    }
    report
}
