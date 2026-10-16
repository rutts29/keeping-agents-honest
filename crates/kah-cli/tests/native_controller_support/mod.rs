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
mod comparison;
#[allow(dead_code)]
pub mod inherited;
mod private_notes;
mod proof;
pub use cleanup::cleanup;
mod empty;
mod history_survival;
mod limits;
mod recovery;
mod steering;
mod usage;
mod verification;
pub use verification::verification_uses_imported_commit;

pub fn run(root: &Path, agent: &str) -> Value {
    if std::env::var_os("KAH_GATE_PRIVATE_NOTES").is_some() {
        return private_notes::run(root, agent);
    }
    if std::env::var_os("KAH_GATE_USAGE").is_some() {
        return usage::run(root, agent);
    }
    if std::env::var_os("KAH_GATE_EMPTY").is_some() {
        return empty::run(root, agent);
    }
    if let Ok(mode) = std::env::var("KAH_GATE_LIMIT") {
        return limits::run(root, agent, &mode);
    }
    if std::env::var_os("KAH_GATE_RECOVERY").is_some() {
        return recovery::run(root, agent);
    }
    if std::env::var_os("KAH_GATE_STEERING").is_some() {
        return steering::run(root, agent);
    }
    completion(root, agent)
}
fn completion(root: &Path, agent: &str) -> Value {
    let home = PathBuf::from(std::env::var_os("HOME").unwrap());
    let before = global_settings(&home);
    let name = format!("kah-native-run-{agent}-{}", std::process::id());
    let domain = if std::env::var_os("KAH_GATE_CONSENT_ONLY").is_some() {
        Domain::Research
    } else {
        Domain::Mine
    };
    let project = fixture_project(&home, &name, domain);
    let model = fixture_model(agent);
    let task = completion_task(agent);
    let output = launch(
        &home,
        &project.root,
        agent,
        model,
        &task,
        proof::VERIFY,
        &[],
    );
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
    if std::env::var_os("KAH_GATE_CONSENT_ONLY").is_some() {
        return consent_result(&home, agent, &record, &before);
    }
    if std::env::var_os("KAH_GATE_COMPLETION_TRIAL").is_some() {
        return comparison::finish(&home, &record, &report, &before);
    }
    capture::capture(&home, &Store::path(&base, id).unwrap(), &record, &report);
    proof::completed(&record, &task);
    let native = record.native.as_ref().unwrap();
    assert!(!project.root.join("double.js").exists());
    proof::native_tools(&home, &Store::path(&base, id).unwrap(), &record);
    assert_eq!(report["tests"]["success"], true);
    for (path, bytes) in before {
        assert_eq!(bytes, fs::read(path).unwrap());
    }
    json!({"agent":agent,"version":version(&home, agent),"fixtureModel":model,"fixtureEffort":"low","sourceDomain":domain.as_str(),"consentRequired":agent=="claude","fullNativeSession":true,"ownHerdrPane":true,"verbatimTask":true,"nativeFileTool":true,"nativeShellTool":true,"nativeTokens":native.native_tokens,"observerTokens":native.observer_tokens,"status":"completed","commit":record.control.last_checkpoint(),"tests":report["tests"],"checkpointReview":checkpoint_proof(&record),"sourceCheckoutUnchanged":true,"globalConfigUnchanged":true,"consentNotificationBeforeFixtureAcceptance":agent=="claude","consentWaitCleared":true,"passed":true})
}
fn checkpoint_proof(record: &Record) -> Value {
    use kah_cli::observer_review::state::Store as Reviews;
    let session = record.native.as_ref().unwrap().session.as_deref().unwrap();
    let state = Reviews::read(session).unwrap().unwrap();
    let checkpoint = state.checkpoint.unwrap();
    assert_eq!(checkpoint["passed"], true);
    assert_eq!(
        checkpoint["head"].as_str(),
        record.control.last_checkpoint()
    );
    let root = Reviews::directory(session).unwrap();
    let result: Value = kah_cli::observer_review::state::read(
        &root
            .join("results")
            .join(format!("{}.json", checkpoint["request"].as_str().unwrap())),
    )
    .unwrap()
    .unwrap();
    assert_eq!(result["assessment"]["passed"], true);
    assert_eq!(result["judgment"]["backend"], "jev");
    json!({"checkpoint":checkpoint,"result":result})
}
fn consent_result(
    home: &Path,
    agent: &str,
    record: &Record,
    before: &[(PathBuf, Vec<u8>)],
) -> Value {
    assert_eq!(record.published_status(), RunStatus::Stopped);
    let native = record.native.as_ref().unwrap();
    assert!(native.waiting.is_none());
    assert!(native.source_cursor > 0 && native.synchronized());
    for (path, bytes) in before {
        assert_eq!(*bytes, fs::read(path).unwrap());
    }
    json!({"agent":agent,"version":version(home, agent),"fixtureModel":record.model,"fixtureEffort":record.effort,"consentNotificationBeforeFixtureAcceptance":true,"consentWaitCleared":true,"primaryTaskAcknowledged":true,"operatorStop":"stopped","globalConfigUnchanged":true,"passed":true})
}
pub(crate) fn version(home: &Path, agent: &str) -> String {
    let output = Command::new(home.join(".local/bin").join(agent))
        .arg("--version")
        .output()
        .unwrap();
    assert!(output.status.success());
    String::from_utf8(output.stdout).unwrap().trim().into()
}
pub(crate) fn fixture_model(agent: &str) -> &'static str {
    match agent {
        "claude" => "claude-sonnet-5-5",
        _ => "gpt-6.1-sol",
    }
}
pub(crate) fn fixture_project(home: &Path, name: &str, domain: Domain) -> Project {
    let project = Project::select(&home.join("repos"), home, Some((domain, name))).unwrap();
    assert!(project.needs_creation);
    project.initialize().unwrap();
    seed(&project.root);
    project
}
fn seed(project: &Path) {
    for args in [
        vec!["config", "user.name", "Native Fixture Agent"],
        vec!["config", "user.email", "fixture@example.invalid"],
        vec!["add", "--all"],
        vec![
            "-c",
            "core.hooksPath=/dev/null",
            "-c",
            "commit.gpgsign=false",
            "commit",
            "--quiet",
            "-m",
            "fixture source baseline",
        ],
    ] {
        assert!(
            Command::new("git")
                .arg("-C")
                .arg(project)
                .args(args)
                .status()
                .unwrap()
                .success()
        );
    }
}
pub(crate) fn global_settings(home: &Path) -> [(PathBuf, Vec<u8>); 2] {
    [
        kah_claude::paths::config_root(home).join("settings.json"),
        home.join(".codex/config.toml"),
    ]
    .map(|p| (p.clone(), fs::read(p).unwrap()))
}
fn pane_screen(pane: &str, session: Option<&str>, agent: &str) -> String {
    kah_cli::run_pane::Pane { id: pane.into() }
        .logs(agent, session, None)
        .unwrap()
}
pub(crate) fn launch(
    home: &Path,
    project: &Path,
    agent: &str,
    model: &str,
    task: &str,
    verify: &str,
    options: &[&str],
) -> String {
    let mut command = Command::new(home.join(".local/bin/kah"));
    if agent == "codex" {
        // This fixture selects the fast tier per launch without global configuration writes.
        command.args(["run", "-c", "service_tier=\"fast\""]);
    } else {
        command.arg("run");
    }
    let result = command
        .args([
            "--agent", agent, "--model", model, "--effort", "low", "--test", verify, task,
        ])
        .args(options)
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
    let mut accepted = false;
    let mut stopped = false;
    let mut observer_consents = std::collections::BTreeSet::new();
    loop {
        let record = Store::read(base, id).unwrap();
        observer_consent(&record, &mut observer_consents);
        if std::env::var_os("KAH_GATE_CONSENT_ONLY").is_some()
            && accepted
            && !stopped
            && record.native.as_ref().unwrap().source_cursor > 0
            && record.native.as_ref().unwrap().waiting.is_none()
        {
            assert!(
                Command::new("kah")
                    .args(["stop", id])
                    .output()
                    .unwrap()
                    .status
                    .success()
            );
            stopped = true;
        }
        if let Some(pane) = record.native.as_ref().unwrap().pane.as_ref() {
            let output = Command::new("herdr")
                .args([
                    "pane", "read", pane, "--source", "recent", "--format", "text",
                ])
                .output()
                .unwrap();
            let screen = String::from_utf8_lossy(&output.stdout);
            if !accepted
                && record.native.as_ref().unwrap().waiting.is_some()
                && (screen.contains("Open restricted")
                    || screen.contains("Yes, I trust this folder"))
            {
                accept_consent(id, pane, agent);
                accepted = true;
            }
            if !matches!(
                record.published_status(),
                RunStatus::Created | RunStatus::Running | RunStatus::Stopping
            ) && Store::path(base, id).unwrap().join("report.json").exists()
            {
                return record;
            }
        }
        std::thread::sleep(Duration::from_millis(300));
    }
}
pub(crate) fn observer_consent(record: &Record, accepted: &mut std::collections::BTreeSet<String>) {
    use kah_cli::observer_review::state::Store as Reviews;
    let Some(session) = record.native.as_ref().and_then(|n| n.session.as_deref()) else {
        return;
    };
    let Ok(Some(state)) = Reviews::read(session) else {
        return;
    };
    let Some(pane) = state.observer.pane else {
        return;
    };
    if state.observer.session.is_some() || accepted.contains(&pane) {
        return;
    }
    assert_eq!(
        state.worker.socket,
        record.native.as_ref().unwrap().herdr_socket
    );
    assert!(
        state
            .worker
            .socket
            .unwrap()
            .to_string_lossy()
            .contains("kah-gate-")
    );
    let output = Command::new("herdr")
        .args([
            "pane", "read", &pane, "--source", "recent", "--format", "text",
        ])
        .output()
        .unwrap();
    let text = String::from_utf8_lossy(&output.stdout);
    if text.contains("Quick safety check:")
        || text.contains("Yes, I trust this folder")
        || text.contains("Open restricted")
    {
        native_consent(&pane, &state.worker.provider);
        accepted.insert(pane);
    }
}
pub(crate) fn accept_consent(id: &str, pane: &str, agent: &str) {
    let status = Command::new("kah").args(["status", id]).output().unwrap();
    assert!(String::from_utf8_lossy(&status.stdout).contains("Needs attention:"));
    native_consent(pane, agent);
}
pub(crate) fn native_consent(pane: &str, agent: &str) {
    std::thread::sleep(Duration::from_millis(400));
    let keys: &[&str] = if agent == "claude" {
        &["Down", "Enter"]
    } else {
        &["Enter"]
    };
    for key in keys {
        send_key(pane, key);
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

fn completion_task(agent: &str) -> String {
    let (file_tool, shell_tool) = if agent == "claude" {
        ("Write", "Bash")
    } else {
        ("apply_patch", "exec_command")
    };
    format!(
        "In this disposable worktree, use {file_tool} to implement double.js: define double(n) returning n * 2 and export that function directly with module.exports = double. Use {shell_tool} to check it with Node. Commit the intended deliverable changes on the run branch, then report. kah copies your HEAD SHA and verifies exactly that commit; kah never authors commits. Do not push or deploy."
    )
}
