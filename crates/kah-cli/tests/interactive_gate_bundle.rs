//! Same Git bundle/fsck/fresh-checkout boundary; no native agent or Jev.
//! git/git v2.53.0 Documentation/git-bundle.adoc and git-worktree.adoc.
use kah_run::worktree::Worktree;
use std::{
    fs,
    path::Path,
    process::{Command, Stdio},
};
fn git(root: &Path, args: &[&str]) -> String {
    let output = Command::new("/usr/bin/git")
        .env_clear()
        .env("PATH", "/usr/bin:/bin")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .args([
            "-c",
            "core.hooksPath=/dev/null",
            "-c",
            "core.fsmonitor=false",
            "-c",
            "user.name=Fixture",
            "-c",
            "user.email=fixture@invalid",
        ])
        .arg("-C")
        .arg(root)
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
fn newer_worker_head_never_changes_the_sha_verified_from_the_bundle() {
    let root = std::env::temp_dir().join(format!(
        "interactive-bundle-{}",
        kah_cli::relay_store::nonce()
    ));
    let source = root.join("source");
    fs::create_dir_all(&source).unwrap();
    git(&source, &["init"]);
    fs::write(source.join("check.cjs"),"require('node:assert/strict').equal(require('fs').readFileSync('value.txt','utf8'),'original');\n").unwrap();
    fs::write(source.join("value.txt"), "original").unwrap();
    git(&source, &["add", "."]);
    git(&source, &["commit", "-m", "initial"]);
    let requested = git(&source, &["rev-parse", "HEAD"]);
    fs::write(source.join("value.txt"), "changed").unwrap();
    git(&source, &["commit", "-am", "later"]);
    let latest = git(&source, &["rev-parse", "HEAD"]);
    let bundle = root.join("source.bundle");
    let output = Command::new(env!("CARGO_BIN_EXE_kah"))
        .args(["__review-gate-bundle", &requested])
        .current_dir(&source)
        .stdout(fs::File::create(&bundle).unwrap())
        .stderr(Stdio::piped())
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let candidate = Worktree::from_bundle(
        &source,
        &root.join("candidate"),
        &bundle,
        &latest,
        "run-gate-bundle-fixture",
    )
    .unwrap();
    let snapshot = candidate
        .verification_snapshot(&root.join("candidate/verification"), &requested)
        .unwrap();
    assert_eq!(snapshot.head().unwrap(), requested);
    assert!(
        Command::new("node")
            .arg("check.cjs")
            .current_dir(&snapshot.path)
            .status()
            .unwrap()
            .success()
    );
    snapshot.remove_verification().unwrap();
    assert_eq!(git(&source, &["rev-parse", "HEAD"]), latest);
    fs::remove_dir_all(root).unwrap();
}
