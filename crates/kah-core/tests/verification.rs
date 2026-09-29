#![cfg(unix)]

use std::fs;
use std::os::unix::fs::PermissionsExt as _;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use kah_core::verification::{
    VerificationError, VerificationRequest, VerificationResult, VerificationStatus, verify,
};

fn temporary_directory(case: &str) -> PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock after epoch")
        .as_nanos();
    let path = std::env::temp_dir().join(format!(
        "kah_core-verification-{case}-{}-{nonce}",
        std::process::id()
    ));
    fs::create_dir(&path).expect("create fixture root");
    path
}

fn git(root: &Path, arguments: &[&str]) {
    let output = Command::new("git")
        .current_dir(root)
        .args(arguments)
        .output()
        .expect("run git");
    assert!(output.status.success(), "git failed: {output:?}");
}

fn candidate(root: &Path) -> PathBuf {
    let candidate = root.join("candidate");
    fs::create_dir(&candidate).expect("create candidate");
    git(&candidate, &["init", "--quiet"]);
    git(
        &candidate,
        &["config", "user.email", "verification@example.invalid"],
    );
    git(&candidate, &["config", "user.name", "Verification"]);
    fs::write(candidate.join("tracked.txt"), "expected\n").expect("write candidate");
    git(&candidate, &["add", "tracked.txt"]);
    git(&candidate, &["commit", "--quiet", "-m", "checkpoint"]);
    candidate
}

fn script(root: &Path, name: &str, body: &str) -> PathBuf {
    let path = root.join(name);
    fs::write(&path, format!("#!/bin/sh\nset -eu\n{body}\n")).expect("write verifier");
    fs::set_permissions(&path, fs::Permissions::from_mode(0o700))
        .expect("make verifier executable");
    path
}

fn run(
    root: &Path,
    candidate: &Path,
    requirement: &str,
    verifier: &Path,
) -> Result<VerificationResult, VerificationError> {
    verify(&VerificationRequest::new(
        candidate,
        requirement,
        [verifier.as_os_str().to_owned()],
        root.join("artifacts"),
    )?)
}

#[test]
fn runs_one_external_requirement_check_and_keeps_raw_output_private() {
    let root = temporary_directory("pass");
    let candidate = candidate(&root);
    let verifier = script(
        &root,
        "pass.sh",
        "test \"$(cat tracked.txt)\" = expected\nprintf public-but-private-output\nprintf diagnostic >&2",
    );
    let result = run(&root, &candidate, "intent-validation-1", &verifier).unwrap();
    assert_eq!(result.status, VerificationStatus::Passed);
    assert_eq!(result.exit_code, Some(0));
    assert_eq!(result.requirement_id, "intent-validation-1");
    assert_eq!(
        result.candidate_before_digest,
        result.candidate_after_digest
    );
    let public = serde_json::to_string(&result).unwrap();
    assert!(!public.contains("public-but-private-output"));
    assert!(!public.contains(&candidate.display().to_string()));
    assert!(!public.contains(&verifier.display().to_string()));
    let artifact = root.join("artifacts").join(&result.artifact_id);
    for directory in [root.join("artifacts"), artifact.clone()] {
        assert_eq!(
            fs::metadata(directory).unwrap().permissions().mode() & 0o077,
            0
        );
    }
    assert_eq!(
        fs::read(artifact.join("stdout.bin")).unwrap(),
        b"public-but-private-output"
    );
    assert_eq!(
        fs::read(artifact.join("stderr.bin")).unwrap(),
        b"diagnostic"
    );
    for name in ["command.json", "stdout.bin", "stderr.bin"] {
        assert_eq!(
            fs::metadata(artifact.join(name))
                .unwrap()
                .permissions()
                .mode()
                & 0o077,
            0
        );
    }
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn distinguishes_command_failure_from_candidate_and_verifier_mutation() {
    let root = temporary_directory("failures");
    let candidate = candidate(&root);
    let failing = script(&root, "fail.sh", "exit 7");
    let failed = run(&root, &candidate, "intent-validation-2", &failing).unwrap();
    assert_eq!(failed.status, VerificationStatus::Failed);
    assert_eq!(failed.exit_code, Some(7));
    let self_mutating = script(&root, "self-mutate.sh", "printf '# changed' >> \"$0\"");
    let protected = run(&root, &candidate, "intent-protected", &self_mutating).unwrap();
    assert_eq!(protected.status, VerificationStatus::ProtectedInputChanged);
    let mutating = script(&root, "mutate.sh", "printf changed > tracked.txt");
    let mutated = run(&root, &candidate, "intent-validation-3", &mutating).unwrap();
    assert_eq!(mutated.status, VerificationStatus::CandidateChanged);
    assert_ne!(
        mutated.candidate_before_digest,
        mutated.candidate_after_digest
    );
    let inside = script(&candidate, "inside-verifier.sh", "exit 0");
    assert!(
        run(&root, &candidate, "intent-inside", &inside)
            .unwrap_err()
            .to_string()
            .contains("verifier inputs must remain outside the candidate")
    );
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn invalid_requirement_has_a_stable_reason_and_stage() {
    let root = temporary_directory("invalid");
    let error = match VerificationRequest::new(&root, "", ["true".into()], root.join("artifacts")) {
        Ok(_) => panic!("empty requirement accepted"),
        Err(error) => error,
    };
    fs::remove_dir_all(root).unwrap();
    assert!(
        error
            .to_string()
            .starts_with("VERIFICATION_REQUIREMENT stage=verification:")
    );
}
