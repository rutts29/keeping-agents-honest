//! The same fsck import boundary used for delivery feeds observer verification.
use kah_run::worktree::Worktree;
use std::{fs, path::Path, process::Command};
fn git(root: &Path, args: &[&str]) -> String {
    let output = Command::new("/usr/bin/git")
        .env_clear()
        .env("PATH", "/usr/bin:/bin")
        .arg("-C")
        .arg(root)
        .args([
            "-c",
            "user.name=Fixture",
            "-c",
            "user.email=fixture@localhost",
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
    String::from_utf8(output.stdout).unwrap().trim().into()
}
#[test]
fn exact_bundle_checkout_excludes_local_files_and_source_git_commands() {
    let root = std::env::temp_dir().join(format!("kah-bundle-verify-{}", std::process::id()));
    fs::create_dir_all(root.join("source")).unwrap();
    let source = root.join("source");
    git(
        &source,
        &["init", "--quiet", "--template=", "--initial-branch=main"],
    );
    fs::write(source.join("check.txt"), "committed").unwrap();
    git(&source, &["add", "check.txt"]);
    git(&source, &["commit", "--quiet", "-m", "fixture agent"]);
    let sha = git(&source, &["rev-parse", "HEAD"]);
    let bundle = root.join("source.bundle");
    git(
        &source,
        &["bundle", "create", bundle.to_str().unwrap(), "HEAD"],
    );
    advance_source(&source);
    assert_ne!(git(&source, &["rev-parse", "HEAD"]), sha);
    fs::write(source.join("check.txt"), "not delivered").unwrap();
    fs::write(source.join("private.txt"), "not delivered").unwrap();
    // A bundle carries objects, never local executable configuration or hooks.
    git(&source, &["config", "core.fsmonitor", "touch HOST_ESCAPE"]);
    let candidate = Worktree::from_bundle(
        &source,
        &root.join("candidate"),
        &bundle,
        &sha,
        "run-observer-fixture",
    )
    .unwrap();
    assert_eq!(candidate.head().unwrap(), sha);
    assert_eq!(
        fs::read_to_string(candidate.path.join("check.txt")).unwrap(),
        "committed"
    );
    assert!(!candidate.path.join("private.txt").exists());
    let fresh = candidate
        .verification_snapshot(&root.join("candidate/verify"), &sha)
        .unwrap();
    assert_eq!(fresh.head().unwrap(), sha);
    assert_eq!(
        fs::read_to_string(fresh.path.join("check.txt")).unwrap(),
        "committed"
    );
    assert!(!source.join("HOST_ESCAPE").exists());
    fresh.remove_verification().unwrap();
    assert!(
        Worktree::from_bundle(
            &source,
            &root.join("wrong"),
            &bundle,
            &"0".repeat(40),
            "run-observer-wrong"
        )
        .is_err()
    );
    assert!(candidate.path.exists());
    fs::remove_dir_all(root).unwrap();
}

fn advance_source(source: &Path) {
    fs::write(source.join("check.txt"), "new agent commit").unwrap();
    git(source, &["add", "check.txt"]);
    git(
        source,
        &["commit", "--quiet", "-m", "later fixture agent commit"],
    );
}
