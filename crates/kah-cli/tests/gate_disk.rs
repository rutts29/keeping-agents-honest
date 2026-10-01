use std::{fs, os::unix::fs::PermissionsExt, path::PathBuf, process::Command};

#[test]
fn low_disk_refuses_a_gate_before_any_build_or_live_journey() {
    let root = std::env::temp_dir().join(format!("kah-disk-{}", std::process::id()));
    fs::create_dir_all(&root).unwrap();
    let marker = root.join("cargo-started");
    for (name, script) in [
        ("df", "#!/bin/sh\nprintf 'Filesystem 1024-blocks Used Available Capacity Mounted\\nfixture 10000000 9999999 1 100%% /\\n'\n".to_owned()),
        ("cargo", format!("#!/bin/sh\ntouch '{}'\nexit 99\n", marker.display())),
    ] {
        let path = root.join(name);
        fs::write(&path, script).unwrap();
        fs::set_permissions(path, fs::Permissions::from_mode(0o700)).unwrap();
    }
    let repository = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let output = Command::new("make")
        .args(["gate", "PHASE=5h-a"])
        .current_dir(&repository)
        .env("CARGO_TARGET_DIR", root.join("target"))
        .env(
            "PATH",
            format!("{}:{}", root.display(), std::env::var("PATH").unwrap()),
        )
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(!marker.exists(), "build started despite low disk");
    assert!(String::from_utf8_lossy(&output.stderr).contains("GATE_DISK_LOW stage=pre_gate"));
    for blocks in ["8388607", "8388608", "invalid"] {
        fs::write(root.join("df"), format!("#!/bin/sh\nprintf 'Filesystem 1024-blocks Used Available Capacity Mounted\\nfixture 10000000 1 {blocks} 1%% /\\n'\n")).unwrap();
        let result = Command::new("bash")
            .arg("demos/check-disk.sh")
            .current_dir(&repository)
            .env("CARGO_TARGET_DIR", root.join("target"))
            .env(
                "PATH",
                format!("{}:{}", root.display(), std::env::var("PATH").unwrap()),
            )
            .output()
            .unwrap();
        assert_eq!(result.status.success(), blocks == "8388608");
    }
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn oversized_target_is_cleaned_before_the_disk_check() {
    let root = std::env::temp_dir().join(format!("kah-prune-{}", std::process::id()));
    fs::create_dir_all(root.join("target")).unwrap();
    let marker = root.join("cargo-args");
    script(
        &root,
        "df",
        "printf 'Filesystem Blocks Used Available Capacity Mounted\\nfixture 9000000 1 8388608 1%% /\\n'",
    );
    script(
        &root,
        "cargo",
        &format!("printf '%s\\n' \"$@\" > '{}'", marker.display()),
    );
    for used in [26214400, 26214401] {
        script(&root, "du", &format!("printf '{used} target\\n'"));
        let result = Command::new("bash")
            .arg("demos/check-disk.sh")
            .current_dir(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../.."))
            .env("CARGO_TARGET_DIR", root.join("target"))
            .env(
                "PATH",
                format!("{}:{}", root.display(), std::env::var("PATH").unwrap()),
            )
            .output()
            .unwrap();
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        assert_eq!(marker.exists(), used > 26214400);
    }
    assert_eq!(
        fs::read_to_string(&marker).unwrap(),
        format!("clean\n--target-dir\n{}\n", root.join("target").display())
    );
    fs::remove_dir_all(root).unwrap();
}
fn script(root: &std::path::Path, name: &str, body: &str) {
    let path = root.join(name);
    fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(0o700)).unwrap();
}
