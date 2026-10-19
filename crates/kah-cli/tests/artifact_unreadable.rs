//! One unreadable file must not fail the whole start inventory.
use serde_json::Value;
use std::{fs, os::unix::fs::PermissionsExt, process::Command};

#[test]
fn baseline_records_an_unreadable_file_as_unsupported_and_keeps_the_rest() {
    let root = std::env::temp_dir().join(format!("kah-artifact-unreadable-{}", std::process::id()));
    fs::create_dir_all(&root).unwrap();
    assert!(
        Command::new("git")
            .args(["init", "--quiet"])
            .arg(&root)
            .status()
            .unwrap()
            .success()
    );
    fs::write(root.join("a.txt"), "first\n").unwrap();
    fs::write(root.join("b.txt"), "second\n").unwrap();
    let locked = root.join("terminal.log");
    fs::write(&locked, "locked\n").unwrap();
    fs::set_permissions(&locked, fs::Permissions::from_mode(0o000)).unwrap();
    if fs::File::open(&locked).is_ok() {
        // A privileged user can read any file, so this case cannot be built.
        fs::remove_dir_all(root).unwrap();
        return;
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
    let blobs = value["blobs"].as_object().unwrap();
    assert_eq!(blobs.len(), 2);
    assert!(blobs.contains_key("a.txt") && blobs.contains_key("b.txt"));
    assert!(!blobs.contains_key("terminal.log"));
    assert_eq!(
        value["unsupported"]["terminal.log"],
        "unreadable before work"
    );
    fs::remove_dir_all(root).unwrap();
}
