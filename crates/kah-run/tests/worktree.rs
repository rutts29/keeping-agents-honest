use kah_run::worktree::Worktree;
use std::{
    fs,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    process::Command,
};
struct Fixture(PathBuf);
#[test]
fn delivery_does_not_commit_unselected_private_files() {
    let f = Fixture::new();
    let worker = Worktree::create(&f.0.join("source"), &f.0.join("worker"), "run-private").unwrap();
    let before = git(&worker.path, &["rev-parse", "HEAD"]);
    fs::create_dir(worker.path.join("notes")).unwrap();
    fs::write(worker.path.join("notes/private.txt"), "PRIVATE_FIXTURE").unwrap();
    assert_eq!(worker.head().unwrap(), before);
    assert!(worker.dirty().unwrap());
    assert!(git(&worker.path, &["status", "--porcelain"]).contains("notes/"));
}
#[test]
fn local_excludes_and_identity_survive_without_following_host_aliases() {
    let f = Fixture::new();
    let source = f.0.join("source");
    fs::create_dir_all(source.join(".git/info")).unwrap();
    fs::write(source.join(".git/info/exclude"), "notes/\n").unwrap();
    git(&source, &["config", "user.name", "Native Fixture Agent"]);
    let worker = Worktree::create(&source, &f.0.join("worker"), "run-excludes").unwrap();
    assert_eq!(
        git(&worker.path, &["config", "--local", "--get", "user.name"]),
        "Native Fixture Agent"
    );
    assert_eq!(
        fs::read_to_string(worker.git_dir.join("info/exclude")).unwrap(),
        "notes/\n"
    );
    fs::create_dir(worker.path.join("notes")).unwrap();
    fs::write(worker.path.join("notes/private.txt"), "PRIVATE_FIXTURE").unwrap();
    assert!(!worker.dirty().unwrap());
    fs::remove_file(source.join(".git/info/exclude")).unwrap();
    std::os::unix::fs::symlink(
        f.0.join("source/value.txt"),
        source.join(".git/info/exclude"),
    )
    .unwrap();
    assert!(
        Worktree::create(&source, &f.0.join("alias"), "run-alias")
            .err()
            .unwrap()
            .contains("RUN_EXCLUDE_READ")
    );
}
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
    let commit = agent_commit(&run);
    fs::write(run.path.join("ignored.txt"), "not part of the branch").unwrap();
    let verification = run
        .verification_snapshot(&run.path.parent().unwrap().join("verification"), &commit)
        .unwrap();
    assert!(!verification.path.join("ignored.txt").exists());
    assert_eq!(verification.head().unwrap(), commit);
    assert!(!verification.dirty().unwrap());
    fs::write(verification.path.join("value.txt"), "changed by test").unwrap();
    assert!(verification.dirty().unwrap());
    assert_eq!(verification.head().unwrap(), commit);
    assert_eq!(run.head().unwrap(), commit);
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
        // A pre-commit hook inherits GIT_INDEX_FILE=.git/index, which is invalid
        // in a linked fixture worktree. Keep the test actor's Git context local.
        .env_clear()
        .env("PATH", "/usr/bin:/bin")
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
fn agent_commit(tree: &Worktree) -> String {
    git(&tree.path, &["add", "--all"]);
    git(
        &tree.path,
        &["commit", "--quiet", "-m", "agent fixture delivery"],
    );
    git(&tree.path, &["rev-parse", "HEAD"])
}
#[test]
fn delivered_history_exposes_added_then_deleted_notes_and_agent_amends() {
    let f = Fixture::new();
    let tree = Worktree::create(&f.0.join("source"), &f.0.join("run"), "run-history").unwrap();
    fs::create_dir(tree.path.join("notes")).unwrap();
    fs::write(tree.path.join("notes/private.txt"), "PRIVATE_FIXTURE").unwrap();
    agent_commit(&tree);
    fs::remove_dir_all(tree.path.join("notes")).unwrap();
    agent_commit(&tree);
    assert!(tree.changed_paths().unwrap().is_empty());
    assert_eq!(tree.commit_evidence().unwrap().len(), 2);
    assert!(
        tree.commit_evidence().unwrap()[0]["changed_paths"]
            .as_array()
            .unwrap()
            .iter()
            .any(|p| p == "notes/private.txt")
    );
    tree.export_branch().unwrap();
    let before = tree.head().unwrap();
    git(
        &tree.path,
        &["commit", "--amend", "-m", "revised agent delivery"],
    );
    assert_ne!(tree.head().unwrap(), before);
    tree.export_branch().unwrap();
    assert_eq!(
        git(&f.0.join("source"), &["rev-parse", "kah/run-history"]),
        tree.head().unwrap()
    );
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
    let commit = agent_commit(&run);
    fs::write(
        run.path.join(".git"),
        format!("gitdir: {}\n", source.join(".git").display()),
    )
    .unwrap();
    assert_eq!(run.head().unwrap(), commit);
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
    assert_eq!(resumed.head().unwrap(), commit);
    fs::write(run.path.join("value.txt"), "unfinished").unwrap();
    fs::write(run.path.join("uncommitted.txt"), "unfinished").unwrap();
    assert!(resumed.dirty().unwrap());
    assert_eq!(resumed.head().unwrap(), commit);
    assert!(run.path.join("uncommitted.txt").exists());
    assert!(resumed.diff_evidence(65536).unwrap().contains("+after"));
    assert!(resumed.diff_evidence(4).is_err());
    assert_eq!(git(&source, &["rev-parse", "HEAD"]), head);
    resumed.export_branch().unwrap();
    assert_eq!(git(&source, &["rev-parse", "kah/run-case"]), commit);
    assert_eq!(git(&source, &["branch", "--show-current"]), "main");
}

#[test]
fn bundles_import_into_separate_trusted_metadata_and_verify_the_commit() {
    let f = Fixture::new();
    let worker = Worktree::create(&f.0.join("source"), &f.0.join("worker"), "run-worker").unwrap();
    let candidate =
        Worktree::create(&f.0.join("source"), &f.0.join("candidate"), "run-candidate").unwrap();
    fs::write(worker.path.join("value.txt"), "worker result").unwrap();
    let commit = agent_commit(&worker);
    let bundle = f.0.join("candidate.bundle");
    git(
        &worker.path,
        &["bundle", "create", bundle.to_str().unwrap(), "HEAD"],
    );
    assert!(candidate.import_bundle(&bundle, &"a".repeat(40)).is_err());
    assert_eq!(
        fs::read_to_string(candidate.path.join("value.txt")).unwrap(),
        "before"
    );
    candidate.import_bundle(&bundle, &commit).unwrap();
    fs::remove_dir_all(&worker.git_dir).unwrap();
    assert_eq!(candidate.head().unwrap(), commit);
    assert_eq!(
        fs::read_to_string(candidate.path.join("value.txt")).unwrap(),
        "worker result"
    );
    assert_eq!(
        fs::read_to_string(f.0.join("source/value.txt")).unwrap(),
        "before"
    );
    let snapshot = candidate
        .verification_snapshot(&f.0.join("candidate/verify"), &commit)
        .unwrap();
    assert_eq!(snapshot.head().unwrap(), commit);
    snapshot.remove_verification().unwrap();
}
