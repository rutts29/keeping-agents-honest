#![cfg(unix)]

use std::fs;
use std::os::unix::fs::PermissionsExt as _;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use kah_core::verification::{VerificationRequest, VerificationResult, VerificationStatus, verify};

fn temporary_directory(case: &str) -> PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock after epoch")
        .as_nanos();
    let path = std::env::temp_dir().join(format!(
        "kah_core-verification-missing-{case}-{}-{nonce}",
        std::process::id()
    ));
    fs::create_dir(&path).expect("create fixture root");
    path
}

fn git(root: &Path, arguments: &[&str]) {
    let output = Command::new("git")
        .current_dir(root)
        .args(["-c", "protocol.file.allow=always"])
        .args(arguments)
        .output()
        .expect("run git");
    assert!(output.status.success(), "git failed: {output:?}");
}

fn repository(path: &Path) {
    fs::create_dir(path).expect("create repository");
    git(path, &["init", "--quiet"]);
    git(
        path,
        &["config", "user.email", "verification@example.invalid"],
    );
    git(path, &["config", "user.name", "Verification"]);
    fs::write(path.join("tracked.txt"), "expected\n").expect("write file");
    git(path, &["add", "tracked.txt"]);
    git(path, &["commit", "--quiet", "-m", "checkpoint"]);
}

fn script(root: &Path, name: &str, body: &str) -> PathBuf {
    let path = root.join(name);
    fs::write(&path, format!("#!/bin/sh\nset -eu\n{body}\n")).expect("write verifier");
    fs::set_permissions(&path, fs::Permissions::from_mode(0o700)).expect("make executable");
    path
}

fn run(root: &Path, candidate: &Path, verifier: &Path) -> VerificationResult {
    verify(
        &VerificationRequest::new(
            candidate,
            "intent-missing",
            [verifier.as_os_str().to_owned()],
            root.join("artifacts"),
        )
        .expect("request"),
    )
    .expect("verification returns a result")
}

#[test]
fn a_verifier_that_deletes_a_tracked_file_gets_candidate_changed_and_keeps_its_output() {
    let root = temporary_directory("delete");
    let candidate = root.join("candidate");
    repository(&candidate);
    let verifier = script(&root, "delete.sh", "printf marker\nrm tracked.txt");

    let result = run(&root, &candidate, &verifier);
    assert_eq!(result.status, VerificationStatus::CandidateChanged);
    assert_ne!(
        result.candidate_before_digest,
        result.candidate_after_digest
    );
    let stdout = root
        .join("artifacts")
        .join(&result.artifact_id)
        .join("stdout.bin");
    assert_eq!(fs::read(stdout).expect("retained output"), b"marker");
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn a_tracked_file_missing_before_the_run_is_a_stable_state() {
    let root = temporary_directory("already-missing");
    let candidate = root.join("candidate");
    repository(&candidate);
    fs::remove_file(candidate.join("tracked.txt")).unwrap();
    let verifier = script(&root, "ok.sh", "exit 0");

    let result = run(&root, &candidate, &verifier);
    assert_eq!(result.status, VerificationStatus::Passed);
    assert_eq!(
        result.candidate_before_digest,
        result.candidate_after_digest
    );
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn a_submodule_is_fingerprinted_instead_of_failing_the_run() {
    let root = temporary_directory("submodule");
    let inner = root.join("inner");
    repository(&inner);
    let candidate = root.join("candidate");
    repository(&candidate);
    git(
        &candidate,
        &[
            "submodule",
            "add",
            "--quiet",
            inner.to_str().unwrap(),
            "sub",
        ],
    );
    git(&candidate, &["commit", "--quiet", "-m", "add submodule"]);

    let steady = script(&root, "steady.sh", "exit 0");
    let result = run(&root, &candidate, &steady);
    assert_eq!(result.status, VerificationStatus::Passed);

    let edits = script(&root, "edit.sh", "printf changed > sub/tracked.txt");
    let result = run(&root, &candidate, &edits);
    assert_eq!(result.status, VerificationStatus::CandidateChanged);
    fs::remove_dir_all(root).unwrap();
}
