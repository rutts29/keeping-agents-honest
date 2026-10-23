use kah_run::{store::valid_run_id, worktree::Worktree};
use std::{fs, path::PathBuf};

fn scratch(case: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!("kah-run-id-{case}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir(&root).unwrap();
    root
}

#[test]
fn one_contract_decides_which_run_ids_are_valid() {
    let long = format!("run-{}", "a".repeat(76));
    assert_eq!(long.len(), 80);
    for valid in ["run-x", "run-20261023-1", "run-A-b-9", long.as_str()] {
        assert!(valid_run_id(valid), "{valid}");
    }
    let too_long = format!("{long}a");
    for invalid in [
        "",
        "run",
        "run-",
        "Run-x",
        "run_x",
        "run-x/y",
        "run-..",
        "run-é",
        too_long.as_str(),
    ] {
        assert!(!valid_run_id(invalid), "{invalid}");
    }
}

#[test]
fn the_bare_prefix_is_refused_before_anything_is_created() {
    let root = scratch("bare");
    let directory = root.join("run-dir");
    let error = Worktree::create(&root.join("missing-source"), &directory, "run-")
        .err()
        .expect("bare prefix refused");
    assert!(error.contains("RUN_ID"), "{error}");
    assert!(!directory.exists());

    let error = Worktree::from_bundle(
        &root.join("missing-source"),
        &directory,
        &root.join("missing.bundle"),
        &"a".repeat(40),
        "run-",
    )
    .err()
    .expect("bare prefix refused");
    assert!(error.contains("RUN_ID"), "{error}");
    assert!(!directory.exists());
    fs::remove_dir_all(root).unwrap();
}
