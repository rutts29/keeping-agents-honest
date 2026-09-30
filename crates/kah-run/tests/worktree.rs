use kah_run::worktree::Worktree;
use std::{
    fs,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    process::Command,
};
struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root =
            std::env::temp_dir().join(format!("kah-worktree-test-{}-{nonce}", std::process::id()));
        fs::create_dir_all(root.join("source")).unwrap();
        git(
            &root.join("source"),
            &["init", "--quiet", "--initial-branch=main", "--template="],
        );
        fs::write(root.join("source/value.txt"), "before").unwrap();
        git(&root.join("source"), &["add", "."]);
        git(&root.join("source"), &["commit", "--quiet", "-m", "base"]);
        Self(root)
    }
}

#[test]
fn verification_starts_from_the_commit_without_ignored_worker_files() {
    let f = Fixture::new();
    let run = Worktree::create(&f.0.join("source"), &f.0.join("run"), "run-verify").unwrap();
    fs::write(run.path.join(".gitignore"), "ignored.txt\n").unwrap();
    let commit = run.checkpoint("ignore generated files").unwrap();
    fs::write(run.path.join("ignored.txt"), "not part of the branch").unwrap();
    let verification = run
        .verification_snapshot(&run.path.parent().unwrap().join("verification"), &commit)
        .unwrap();
    assert!(!verification.path.join("ignored.txt").exists());
    assert_eq!(verification.checkpoint("unchanged").unwrap(), commit);
    fs::write(verification.path.join("value.txt"), "changed by test").unwrap();
    assert_ne!(verification.checkpoint("changed").unwrap(), commit);
    assert_eq!(run.checkpoint("source unchanged").unwrap(), commit);
    let path = verification.path.clone();
    verification.remove_verification().unwrap();
    assert!(!path.exists());
    assert!(run.path.exists());
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn git(cwd: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .arg("-C")
        .arg(cwd)
        .args([
            "-c",
            "user.name=Test",
            "-c",
            "user.email=test@localhost",
            "-c",
            "commit.gpgsign=false",
            "-c",
            "core.hooksPath=/dev/null",
        ])
        .args(args)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8(out.stdout).unwrap().trim().into()
}
#[test]
fn private_worktree_ignores_source_hooks_filters_and_a_redirected_git_file() {
    let f = Fixture::new();
    let source = f.0.join("source");
    let canary = f.0.join("host-canary");
    let script = format!("#!/bin/sh\nprintf UNSAFE > '{}'\ncat\n", canary.display());
    let hook = source.join(".git/hooks/post-checkout");
    fs::create_dir_all(hook.parent().unwrap()).unwrap();
    fs::write(&hook, &script).unwrap();
    fs::set_permissions(&hook, fs::Permissions::from_mode(0o755)).unwrap();
    fs::write(source.join(".gitattributes"), "*.txt filter=hostile\n").unwrap();
    git(&source, &["add", ".gitattributes"]);
    git(&source, &["commit", "--quiet", "-m", "attributes"]);
    git(
        &source,
        &["config", "filter.hostile.smudge", hook.to_str().unwrap()],
    );
    let head = git(&source, &["rev-parse", "HEAD"]);
    let run = Worktree::create(&source, &f.0.join("run"), "run-case").unwrap();
    assert!(!canary.exists());
    assert_eq!(run.base_commit, head);
    assert_eq!(
        git(&run.path, &["branch", "--show-current"]),
        "kah/run-case"
    );
    assert_eq!(git(&run.git_dir, &["remote"]), "");
    fs::write(run.path.join("value.txt"), "after").unwrap();
    fs::write(
        run.path.join(".git"),
        format!("gitdir: {}\n", source.join(".git").display()),
    )
    .unwrap();
    let commit = run.checkpoint("checkpoint").unwrap();
    assert_ne!(commit, head);
    assert_eq!(git(&source, &["rev-parse", "HEAD"]), head);
    assert_eq!(
        fs::read_to_string(source.join("value.txt")).unwrap(),
        "before"
    );
    assert!(!canary.exists());
    let saved = f.0.join("worktree.json");
    run.save(&saved).unwrap();
    let resumed = Worktree::load(&saved).unwrap();
    assert_eq!(resumed.checkpoint("unchanged resume").unwrap(), commit);
    fs::write(run.path.join("value.txt"), "unfinished").unwrap();
    fs::write(run.path.join("uncommitted.txt"), "unfinished").unwrap();
    resumed.restore_checkpoint(&commit).unwrap();
    assert_eq!(
        fs::read_to_string(run.path.join("value.txt")).unwrap(),
        "after"
    );
    assert!(!run.path.join("uncommitted.txt").exists());
    assert!(resumed.diff_evidence(65536).unwrap().contains("+after"));
    assert!(resumed.diff_evidence(4).is_err());
    assert_eq!(git(&source, &["rev-parse", "HEAD"]), head);
    resumed.export_branch().unwrap();
    assert_eq!(git(&source, &["rev-parse", "kah/run-case"]), commit);
    assert_eq!(git(&source, &["branch", "--show-current"]), "main");
}
