//! Controller receipts classify retired mechanisms; model stdout is not authority.
use std::{fs, os::unix::fs::PermissionsExt, path::Path, process::Command};
fn run(label: &str, receipt: &str, body: &str) -> (std::path::PathBuf, std::process::Output) {
    let root = std::env::temp_dir().join(format!(
        "kah-retired-gate-unit-{label}-{}",
        std::process::id()
    ));
    fs::create_dir_all(root.join("demos/phase-0")).unwrap();
    fs::create_dir_all(root.join("bin")).unwrap();
    fs::write(root.join("demos/check-disk.sh"), "exit 0\n").unwrap();
    fs::write(root.join("demos/phase-0/superseded.json"), receipt).unwrap();
    fs::write(root.join("demos/phase-0/retired.sh"), body).unwrap();
    fs::write(root.join("bin/cargo"), "#!/bin/sh\nexit 0\n").unwrap();
    fs::set_permissions(root.join("bin/cargo"), fs::Permissions::from_mode(0o700)).unwrap();
    let script = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../demos/run-gate.sh");
    let output = Command::new("sh")
        .arg(script)
        .arg("0")
        .current_dir(&root)
        .env(
            "PATH",
            format!("{}:/usr/bin:/bin", root.join("bin").display()),
        )
        .env("KAH_GATES_KEEP_GOING", "1")
        .env("KAH_GATES_RESULTS_DIR", root.join("results"))
        .env_remove("KAH_GATE_DEMO")
        .output()
        .unwrap();
    (root, output)
}
const RECEIPT: &str = r#"{"phase":"0","status":"superseded","passed":null,"authority":[{"tracker":39}],"replaced_by":["current proof"]}"#;
#[test]
fn a_current_verified_supersession_is_recorded_separately_from_pass() {
    let (root, output) = run(
        "retired",
        RECEIPT,
        "cp demos/phase-0/superseded.json \"$KAH_GATE_OUTCOME_RECORD\"; echo SUPERSEDED\n",
    );
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        fs::read_to_string(root.join("results/results.tsv"))
            .unwrap()
            .starts_with("superseded\t")
    );
    fs::remove_dir_all(root).unwrap();
}
#[test]
fn a_foreign_or_changed_receipt_never_becomes_a_pass() {
    let (root, output) = run(
        "forged",
        RECEIPT,
        "printf '{\"status\":\"superseded\",\"passed\":null}' > \"$KAH_GATE_OUTCOME_RECORD\"\n",
    );
    assert!(!output.status.success());
    assert!(
        fs::read_to_string(root.join("results/results.tsv"))
            .unwrap()
            .starts_with("fail\t")
    );
    fs::remove_dir_all(root).unwrap();
}
