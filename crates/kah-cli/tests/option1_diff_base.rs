//! 5h-d1 (2026-10-18): a dispatched kah run worker had no baseline file (its
//! first HEAD sample saw no HEAD), so its checkpoint diffed against the empty
//! tree and counted the fixture's committed AGENTS.md and CLAUDE.md as new.
use kah_cli::observer_review::option1::diff_base::diff_base;
use serde_json::json;
use std::{path::Path, process::Command};

fn git(dir: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args([
            "-c",
            "user.name=fixture",
            "-c",
            "user.email=fixture@example.invalid",
        ])
        .args([
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
        "git {args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap().trim().to_owned()
}

#[test]
fn an_kah_run_worker_without_a_baseline_diffs_from_the_runs_base_commit() {
    let home = std::env::temp_dir().join(format!("diff-base-{}", std::process::id()));
    let source = home.join("source");
    std::fs::create_dir_all(&source).unwrap();
    git(&source, &["init", "--quiet", "-b", "main"]);
    std::fs::write(source.join("AGENTS.md"), "# Project working agreement\n").unwrap();
    std::os::unix::fs::symlink("AGENTS.md", source.join("CLAUDE.md")).unwrap();
    git(&source, &["add", "--all"]);
    git(&source, &["commit", "--quiet", "-m", "fixture baseline"]);
    let id = format!("run-diff-base-{}", std::process::id());
    let tree = kah_run::worktree::Worktree::create(&source, &home.join(&id), &id).unwrap();
    let base = kah_core::run_store::RunStore::default_state_root()
        .unwrap()
        .join("runs");
    let record = kah_run::store::Record {
        project: source.clone(),
        domain: "research".into(),
        worktree: tree.path.clone(),
        model: None,
        effort: None,
        native: Some(kah_run::native::Native::new("claude").unwrap()),
        control: kah_core::unattended::RunControl::new("fixture", Default::default()).unwrap(),
    };
    let store = kah_run::store::Store::create(&base, &id, record).unwrap();
    drop(store);
    tree.save(
        &kah_run::store::Store::path(&base, &id)
            .unwrap()
            .join("worktree.json"),
    )
    .unwrap();
    // The worker commits only marker.txt.
    std::fs::write(tree.path.join("marker.txt"), "BLUE\n").unwrap();
    git(&tree.path, &["add", "marker.txt"]);
    git(&tree.path, &["commit", "--quiet", "-m", "marker"]);

    let chosen = diff_base(None, &tree.path);
    assert_eq!(
        chosen.as_deref(),
        Some(tree.base_commit.as_str()),
        "no baseline: the run's base commit"
    );
    let changed = git(
        &tree.path,
        &["diff", "--name-only", chosen.as_deref().unwrap(), "HEAD"],
    );
    assert_eq!(
        changed, "marker.txt",
        "the fixture's committed files are not new"
    );
    // A recorded baseline wins; an interactive project has no run base.
    let recorded = json!({"head":"0123456789abcdef0123456789abcdef01234567"});
    assert_eq!(
        diff_base(Some(&recorded), &tree.path).as_deref(),
        recorded["head"].as_str()
    );
    assert_eq!(diff_base(None, &source), None);

    std::fs::remove_dir_all(kah_run::store::Store::path(&base, &id).unwrap()).unwrap();
    let _ = std::fs::remove_dir_all(home);
}
