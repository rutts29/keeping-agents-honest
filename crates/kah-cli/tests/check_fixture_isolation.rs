//! Test the actual check wrapper's failed-command cleanup, not a mock store.
use std::{fs, path::PathBuf, process::Command};

const FAILING_CHILD: &str = "printf '%s\\n' \"$XDG_STATE_HOME\"; mkdir -p \"$XDG_STATE_HOME/kah/fixture\"; printf data > \"$XDG_STATE_HOME/kah/fixture/retained\"; exit 7";

fn assert_failed_child_is_cleaned_up(name: &str) {
    let base = std::env::temp_dir().join(format!("{name}-{}", kah_cli::relay_store::nonce()));
    fs::create_dir_all(&base).unwrap();
    let base = fs::canonicalize(base).unwrap();
    let output = Command::new("bash")
        .arg("demos/check-tests.sh")
        .args(["sh", "-c", FAILING_CHILD])
        .env("KAH_TEST_FIXTURE_BASE", &base)
        .current_dir(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../.."))
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(7));
    let state = PathBuf::from(String::from_utf8(output.stdout).unwrap().trim());
    assert!(state.starts_with(&base));
    assert!(!state.exists());
    assert_eq!(fs::read_dir(&base).unwrap().count(), 0);
    fs::remove_dir_all(base).unwrap();
}

#[test]
fn failed_check_command_cleans_its_run_state_and_keeps_parent_status() {
    assert_failed_child_is_cleaned_up("wrapper-test");
}

#[test]
fn a_fixture_base_containing_spaces_is_kept_and_only_its_run_is_removed() {
    assert_failed_child_is_cleaned_up("wrapper test with spaces");
}
