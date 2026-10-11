//! Harness-only behavior: no native agent or provider assumptions/calls.
use std::{fs, os::unix::fs::PermissionsExt, path::PathBuf, process::Command};

fn fixture() -> PathBuf {
    let root = std::env::temp_dir().join(format!("gate-runner-{}", kah_cli::relay_store::nonce()));
    for directory in ["demos/phase-fixture", "bin"] {
        fs::create_dir_all(root.join(directory)).unwrap();
    }
    fs::write(
        root.join("demos/run-gate.sh"),
        include_str!("../../../demos/run-gate.sh"),
    )
    .unwrap();
    fs::write(root.join("demos/check-disk.sh"), "echo disk >> events\n").unwrap();
    fs::write(
        root.join("demos/phase-fixture/a.sh"),
        "echo first >> events\nexit 7\n",
    )
    .unwrap();
    fs::write(
        root.join("demos/phase-fixture/b.sh"),
        "echo second >> events\n",
    )
    .unwrap();
    let cargo = root.join("bin/cargo");
    fs::write(&cargo, "#!/bin/sh\necho audit >> events\nexit 0\n").unwrap();
    fs::set_permissions(cargo, fs::Permissions::from_mode(0o700)).unwrap();
    root
}
fn run(root: &PathBuf, keep_going: bool) -> std::process::Output {
    let path = format!(
        "{}:{}",
        root.join("bin").display(),
        std::env::var("PATH").unwrap()
    );
    Command::new("bash")
        .args(["demos/run-gate.sh", "fixture"])
        .current_dir(root)
        .env("PATH", path)
        .env("KAH_GATES_KEEP_GOING", if keep_going { "1" } else { "0" })
        .env("KAH_GATES_RESULTS_DIR", root.join("results"))
        .output()
        .unwrap()
}
#[test]
fn strict_default_stops_at_first_failed_demo_and_audits() {
    let root = fixture();
    let output = run(&root, false);
    assert_eq!(output.status.code(), Some(7));
    assert_eq!(
        fs::read_to_string(root.join("events")).unwrap(),
        "disk\ndisk\nfirst\naudit\n"
    );
    assert!(!root.join("results").exists());
    fs::remove_dir_all(root).unwrap();
}
#[test]
fn keep_going_runs_all_demos_with_disk_checks_audits_and_full_failure_list() {
    let root = fixture();
    let output = run(&root, true);
    assert_eq!(output.status.code(), Some(1));
    assert_eq!(
        fs::read_to_string(root.join("events")).unwrap(),
        "disk\ndisk\nfirst\naudit\ndisk\nsecond\naudit\naudit\n"
    );
    let results = fs::read_to_string(root.join("results/results.tsv")).unwrap();
    assert!(results.contains("fail\tdemos/phase-fixture/a.sh\t7\t0\t"));
    assert!(results.contains("pass\tdemos/phase-fixture/b.sh\t0\t0\t"));
    assert_eq!(results.lines().count(), 2);
    assert!(String::from_utf8(output.stdout).unwrap().contains(&results));
    assert!(root.join("results/phase-fixture_a.sh.log").exists());
    fs::remove_dir_all(root).unwrap();
}
