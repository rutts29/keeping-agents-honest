//! $0 entrypoint contract; fake cargo/disk checks start no agents or containers.
use std::{fs, os::unix::fs::PermissionsExt, path::Path, process::Command};
fn fixture(label: &str) -> std::path::PathBuf {
    let root = std::env::temp_dir().join(format!(
        "kah-gate-select-unit-{label}-{}",
        std::process::id()
    ));
    fs::create_dir_all(root.join("demos/phase-0")).unwrap();
    fs::create_dir_all(root.join("bin")).unwrap();
    fs::write(root.join("demos/check-disk.sh"), "exit 0\n").unwrap();
    fs::write(
        root.join("demos/phase-0/good.sh"),
        "printf 'selected\\n' > selected\n",
    )
    .unwrap();
    fs::write(
        root.join("demos/phase-0/bad.sh"),
        "printf 'wrong\\n' > wrong; exit 7\n",
    )
    .unwrap();
    fs::write(root.join("bin/cargo"), "#!/bin/sh\nexit 0\n").unwrap();
    fs::set_permissions(root.join("bin/cargo"), fs::Permissions::from_mode(0o700)).unwrap();
    root
}
fn run(root: &Path, demo: &str) -> std::process::Output {
    let script = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../demos/run-gate.sh");
    Command::new("sh")
        .arg(script)
        .arg("0")
        .current_dir(root)
        .env(
            "PATH",
            format!("{}:/usr/bin:/bin", root.join("bin").display()),
        )
        .env("KAH_GATE_DEMO", demo)
        .env_remove("KAH_GATES_KEEP_GOING")
        .output()
        .unwrap()
}
#[test]
fn selected_failed_demo_runs_without_repeating_other_cases() {
    let root = fixture("selected");
    let output = run(&root, "demos/phase-0/good.sh");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(root.join("selected").exists());
    assert!(!root.join("wrong").exists());
    fs::remove_dir_all(root).unwrap();
}
#[test]
fn a_selector_cannot_escape_the_requested_phase() {
    let root = fixture("escape");
    let output = run(&root, "demos/phase-0/../phase-0/good.sh");
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("GATE_DEMO_SCOPE"));
    assert!(!root.join("selected").exists());
    fs::remove_dir_all(root).unwrap();
}
