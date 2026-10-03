use std::{fs, path::Path, process::Command};

fn git(root: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .current_dir(root)
        .args([
            "-c",
            "user.name=kah fixture",
            "-c",
            "user.email=fixture@example.invalid",
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
    String::from_utf8(output.stdout).unwrap()
}

#[test]
fn cumulative_context_includes_older_committed_code_and_current_changes() {
    let root = std::env::temp_dir().join(format!("kah-review-context-{}", std::process::id()));
    fs::create_dir_all(&root).unwrap();
    git(&root, &["init", "--quiet"]);
    fs::write(root.join("phase-one.txt"), "first phase implementation\n").unwrap();
    git(&root, &["add", "."]);
    git(&root, &["commit", "--quiet", "-m", "fixture phase one"]);
    fs::write(root.join("phase-two.txt"), "second phase implementation\n").unwrap();
    git(&root, &["add", "."]);
    git(&root, &["commit", "--quiet", "-m", "fixture phase two"]);
    let head = git(&root, &["rev-parse", "HEAD"]);
    fs::write(root.join("phase-two.txt"), "uncommitted correction\n").unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_kah"))
        .arg("__review-project-context")
        .current_dir(&root)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let context: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(context["head"], head.trim());
    let committed = context["committed_snapshot"].as_str().unwrap_or("");
    assert!(committed.contains("first phase implementation"));
    assert!(committed.contains("second phase implementation"));
    assert!(!committed.contains("uncommitted correction"));
    assert!(
        context["diff"]
            .as_str()
            .unwrap()
            .contains("uncommitted correction")
    );
    assert_eq!(git(&root, &["rev-parse", "HEAD"]), head);
    fs::remove_dir_all(root).unwrap();
}
