use kah_cli::observer_review::{hook, state::Store};

#[test]
fn detached_jobs_use_installed_kah_instead_of_the_callers_executable() {
    let worker = format!("job-program-{}", std::process::id());
    let store = Store::open(&worker).unwrap();
    let root = store.directory.clone();
    drop(store);
    hook::spawn("__review-consume", &worker).unwrap();
    let log = root.join("job-__review-consume.log");
    // This is a host-only missing-state command, not a native/model session.
    // An old deleted executable or the Rust test runner cannot execute it.
    let lock = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(root.join("job-__review-consume.lock"))
        .unwrap();
    lock.lock().unwrap();
    let output = std::fs::read_to_string(log).unwrap();
    assert!(
        output.contains("REVIEW_STATE_MISSING"),
        "installed kah did not process the detached command: {output}"
    );
    std::fs::remove_dir_all(root.parent().unwrap()).unwrap();
}
