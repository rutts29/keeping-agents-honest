//! Development probe. Gate 4 still requires the complete CLI and observer loop.
use kah_core::unattended::{Limits, RunControl};
use kah_run::{
    access, attempt,
    runtime::{Runtime, service_uid},
    store::{Record, Store},
    worktree::Worktree,
};
use std::{
    fs,
    os::unix::fs::PermissionsExt,
    path::PathBuf,
    process::Command,
    time::{SystemTime, UNIX_EPOCH},
};

#[test]
#[ignore = "real rootless gVisor and native model; development probe only"]
fn mine_worker_edits_then_checkpoints_after_a_budget_stop() {
    let (root, source, tree) = fixture();
    execute_probe(&root, &source, &tree);
    fs::remove_dir_all(root).unwrap();
}

fn fixture() -> (PathBuf, PathBuf, Worktree) {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root = PathBuf::from(format!("/var/tmp/kah-run-live-{nonce}"));
    fs::create_dir(&root).unwrap();
    fs::set_permissions(&root, fs::Permissions::from_mode(0o700)).unwrap();
    access::traverse(&root, service_uid().unwrap()).unwrap();
    let source = root.join("source");
    fs::create_dir(&source).unwrap();
    git(
        &source,
        &["init", "--quiet", "--template=", "--initial-branch=main"],
    );
    fs::write(source.join("answer.txt"), "BEFORE\n").unwrap();
    git(&source, &["add", "."]);
    git(&source, &["commit", "--quiet", "-m", "base"]);
    let tree = Worktree::create(&source, &root.join("job"), "run-probe").unwrap();
    access::traverse(&root.join("job"), service_uid().unwrap()).unwrap();
    access::share_tree(&tree.path, service_uid().unwrap(), true).unwrap();
    access::share_tree(&tree.git_dir, service_uid().unwrap(), false).unwrap();
    (root, source, tree)
}

fn execute_probe(root: &std::path::Path, source: &std::path::Path, tree: &Worktree) {
    let task = "Use native apply_patch to replace answer.txt with AFTER followed by a newline. Run pwd and git status --short. Do not commit or install anything. Stop after the edit.";
    let mut control = RunControl::new(
        task,
        Limits {
            tokens: Some(1),
            seconds: Some(120),
        },
    )
    .unwrap();
    control.start().unwrap();
    let record = Record {
        native: None,
        project: source.into(),
        domain: "mine".into(),
        worktree: tree.path.clone(),
        model: Some("gpt-6-sol".into()),
        effort: Some("low".into()),
        control,
    };
    let state_base = root.join("state");
    let store = Store::create(&state_base, "run-probe", record.clone()).unwrap();
    let home = PathBuf::from(std::env::var_os("HOME").unwrap());
    let runtime = Runtime::prepare(&home, store.directory(), "run-probe", &record, tree).unwrap();
    let log = store.directory().join("attempt-1.log");
    drop(store);
    let outcome = attempt::run(
        &state_base,
        "run-probe",
        runtime.command(&record),
        task,
        &log,
    );
    runtime.stop_executor("run-probe").unwrap();
    let outcome = outcome.unwrap();
    assert_eq!(
        outcome.stopped.as_deref(),
        Some("RUN_TOKEN_BUDGET"),
        "log {}",
        log.display()
    );
    verify_checkpoint(root, source, tree, &runtime, outcome);
}

fn verify_checkpoint(
    root: &std::path::Path,
    source: &std::path::Path,
    tree: &Worktree,
    runtime: &Runtime,
    outcome: attempt::Outcome,
) {
    assert_eq!(
        fs::read_to_string(tree.path.join("answer.txt")).unwrap(),
        "AFTER\n"
    );
    assert_eq!(
        fs::read_to_string(source.join("answer.txt")).unwrap(),
        "BEFORE\n"
    );
    let commit = tree.checkpoint("feat: update answer").unwrap();
    let receipt = kah_run::verification::execute(
        &runtime.executor,
        "test \"$(cat answer.txt)\" = AFTER",
        &commit,
        std::time::Duration::from_secs(20),
        &root.join("test.log"),
    )
    .unwrap();
    assert!(receipt.success, "{}", receipt.output);
    assert_eq!(tree.checkpoint("after verification").unwrap(), commit);
    let mut store = Store::open(&root.join("state"), "run-probe").unwrap();
    let mut record = store.load().unwrap();
    record.control.checkpoint(commit.clone()).unwrap();
    store.save(&record).unwrap();
    tree.export_branch().unwrap();
    eprintln!(
        "{}",
        serde_json::json!({"developmentProbe":true,"domain":"mine","commit":commit,"nativeThread":outcome.thread,"actions":outcome.actions,"sourceUnchanged":true,"tests":receipt,"budgetStopped":record.control.status()})
    );
}
fn git(path: &std::path::Path, args: &[&str]) {
    let output = Command::new("/usr/bin/git")
        .arg("-C")
        .arg(path)
        .args([
            "-c",
            "user.name=kah",
            "-c",
            "user.email=kah@localhost",
            "-c",
            "commit.gpgsign=false",
            "-c",
            "core.hooksPath=/dev/null",
        ])
        .args(args)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}
