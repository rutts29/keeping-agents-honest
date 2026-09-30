use kah_core::unattended::{Limits, RunControl};
use kah_run::{
    commands,
    lease::Lease,
    store::{Record, Store},
};
use std::{fs, path::PathBuf};
#[test]
fn worker_lease_is_exclusive_but_does_not_block_operator_steering() {
    let root = std::env::temp_dir().join(format!("kah-run-lease-{}", std::process::id()));
    let record = Record {
        project: PathBuf::from("/project"),
        domain: "mine".into(),
        worktree: PathBuf::from("/worktree"),
        model: None,
        effort: None,
        control: RunControl::new("task", Limits::default()).unwrap(),
    };
    drop(Store::create(&root, "run-test", record).unwrap());
    let lease = Lease::acquire(&root, "run-test").unwrap();
    assert!(Lease::acquire(&root, "run-test").is_err());
    assert_eq!(
        commands::steer(&root, "run-test", "Keep the API").unwrap(),
        1
    );
    drop(lease);
    drop(Lease::acquire(&root, "run-test").unwrap());
    fs::remove_dir_all(root).unwrap();
}
