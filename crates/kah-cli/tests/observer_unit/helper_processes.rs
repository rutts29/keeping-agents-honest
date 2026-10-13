use super::*;

#[test]
fn process_receipt_hashes_the_opened_executable_and_records_its_start_stamp() {
    let pid = std::process::id();
    let root = std::path::PathBuf::from(format!("/proc/{pid}"));
    let record = snapshot(pid, "receipt-test", &root).unwrap();
    let expected = format!("{:x}", Sha256::digest(fs::read(root.join("exe")).unwrap()));
    assert_eq!(record["exe_sha256"], expected);
    assert_eq!(record["pid"], pid);
    assert!(!record["process_stamp"].as_str().unwrap().is_empty());
}

#[test]
fn daemon_registry_excludes_foreground_sessions_and_finite_transactions() {
    for command in [
        "__review-watch",
        "__review-liveness-watch",
        "__attach-observer",
        "__untrusted-commits",
    ] {
        assert!(crate::helper_upgrade::long_lived(std::ffi::OsStr::new(
            command
        )));
    }
    for command in [
        "run",
        "resume",
        "__review-consume-worker",
        "__gate-host",
        "__review-hook-events",
    ] {
        assert!(!crate::helper_upgrade::long_lived(std::ffi::OsStr::new(
            command
        )));
    }
}
