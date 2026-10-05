//! Real native Git reads, no commits or model sessions.
use serde_json::Value;
use std::{fs, process::Command};

#[test]
fn baseline_hashes_every_quoted_path_without_writing_git_objects_or_commits() {
    let root =
        std::env::temp_dir().join(format!("kah-artifact-native-unit-{}", std::process::id()));
    fs::create_dir_all(&root).unwrap();
    assert!(
        Command::new("git")
            .args(["init", "--quiet"])
            .arg(&root)
            .status()
            .unwrap()
            .success()
    );
    let names = ["PLAN.md", "test with spaces.cjs", "test\nwith newline.cjs"];
    for name in names {
        fs::write(root.join(name), "unchanged test bytes\n").unwrap();
    }
    let output = Command::new(env!("CARGO_BIN_EXE_kah"))
        .arg("__review-artifact-baseline")
        .current_dir(&root)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let value: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["blobs"].as_object().unwrap().len(), 3);
    let expected = Command::new("git")
        .args(["hash-object", "--no-filters", "PLAN.md"])
        .current_dir(&root)
        .output()
        .unwrap();
    let expected = String::from_utf8(expected.stdout).unwrap();
    for name in names {
        assert_eq!(value["blobs"][name], expected.trim());
    }
    assert!(
        !Command::new("git")
            .args(["rev-parse", "--verify", "HEAD"])
            .current_dir(&root)
            .output()
            .unwrap()
            .status
            .success()
    );
    assert_eq!(fs::read_dir(root.join(".git/objects")).unwrap().count(), 2);
    fs::remove_dir_all(root).unwrap();
}
