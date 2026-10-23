//! Completion evidence is read through a byte budget, never whole.
use kah_run::worktree::Worktree;
use std::{
    fs,
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
            std::env::temp_dir().join(format!("kah-evidence-test-{}-{nonce}", std::process::id()));
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
    fn candidate(&self, files: &[(&str, Vec<u8>)]) -> Worktree {
        let tree =
            Worktree::create(&self.0.join("source"), &self.0.join("run"), "run-evidence").unwrap();
        for (name, bytes) in files {
            fs::write(tree.path.join(name), bytes).unwrap();
        }
        git(&tree.path, &["add", "--all"]);
        git(
            &tree.path,
            &["commit", "--quiet", "-m", "agent fixture delivery"],
        );
        tree
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn git(cwd: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
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

fn text(bytes: usize) -> Vec<u8> {
    "line of fixture text\n"
        .bytes()
        .cycle()
        .take(bytes)
        .collect()
}

fn limit_error(result: Result<impl std::fmt::Debug, String>) {
    let error = result.unwrap_err();
    assert!(
        error.starts_with("RUN_EVIDENCE_LIMIT stage=completion"),
        "{error}"
    );
}

#[test]
fn a_large_committed_file_is_refused_not_read_whole() {
    let f = Fixture::new();
    let tree = f.candidate(&[("big.txt", text(5 * 1024 * 1024))]);
    limit_error(tree.diff_evidence());
    limit_error(tree.file_evidence());
}

#[test]
fn evidence_that_fits_its_budget_exactly_is_kept_whole() {
    let f = Fixture::new();
    let tree = f.candidate(&[("a.txt", text(300)), ("b.txt", text(200))]);
    let diff = tree.diff_evidence().unwrap();
    assert_eq!(tree.diff_evidence_within(diff.len()).unwrap(), diff);
    limit_error(tree.diff_evidence_within(diff.len() - 1));
    let files = tree.file_evidence_within(500).unwrap();
    assert_eq!(files.len(), 2);
    assert!(files.iter().all(|file| file["complete"] == true));
    assert_eq!(files[0]["content"].as_str().unwrap().len(), 300);
}

#[test]
fn files_that_fit_alone_but_not_together_are_refused() {
    let f = Fixture::new();
    let tree = f.candidate(&[("a.txt", text(300)), ("b.txt", text(200))]);
    limit_error(tree.file_evidence_within(499));
    limit_error(tree.file_evidence_within(299));
}

#[test]
fn a_large_binary_file_is_refused_before_it_is_judged_binary() {
    let f = Fixture::new();
    let tree = f.candidate(&[("blob.bin", vec![0u8; 400])]);
    limit_error(tree.file_evidence_within(399));
    let files = tree.file_evidence_within(400).unwrap();
    assert_eq!(
        files[0]["complete"], false,
        "a NUL byte marks it as no text"
    );
}

#[test]
fn the_path_and_commit_inventories_have_the_same_budget() {
    let f = Fixture::new();
    let names: Vec<(String, Vec<u8>)> = (0..40)
        .map(|n| (format!("file-{n:03}.txt"), text(5)))
        .collect();
    let files: Vec<(&str, Vec<u8>)> = names.iter().map(|(n, b)| (n.as_str(), b.clone())).collect();
    let tree = f.candidate(&files);
    assert_eq!(tree.changed_paths().unwrap().len(), 40);
    assert_eq!(tree.commit_evidence().unwrap().len(), 1);
    let paths_bytes: usize = names.iter().map(|(n, _)| n.len() + 1).sum();
    assert_eq!(tree.changed_paths_within(paths_bytes).unwrap().len(), 40);
    limit_error(tree.changed_paths_within(paths_bytes - 1));
    limit_error(tree.commit_evidence_within(paths_bytes));
}
