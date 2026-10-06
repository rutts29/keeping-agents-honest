use super::*;

#[test]
fn rejects_a_second_writer_with_a_deterministic_busy_error() {
    let (root, repository, store, _) = create_store("writer-lock");
    let error = RunStore::open(&root, &repository, "run_01").expect_err("second writer is busy");
    assert_eq!(error, RunStoreError::Busy);
    drop(store);
    RunStore::open(&root, &repository, "run_01").expect("lock is released on drop");

    fs::remove_dir_all(root).expect("remove isolated test directory");
}

#[test]
fn lock_holder_subprocess() {
    if std::env::var_os("DRIFTCTL_RUN_STORE_LOCK_CHILD").is_none() {
        return;
    }
    let root =
        PathBuf::from(std::env::var_os("DRIFTCTL_RUN_STORE_STATE_ROOT").expect("state root"));
    let repository =
        PathBuf::from(std::env::var_os("DRIFTCTL_RUN_STORE_REPOSITORY").expect("repository"));
    let ready = PathBuf::from(std::env::var_os("DRIFTCTL_RUN_STORE_READY").expect("ready path"));
    let _store = RunStore::open(root, repository, "run_01").expect("child acquires writer lock");
    fs::write(ready, "locked\n").expect("signal acquired lock");
    loop {
        thread::sleep(std::time::Duration::from_secs(1));
    }
}

#[test]
fn writer_lock_is_released_after_a_holder_process_is_killed() {
    let (root, repository, store, _) = create_store("writer-lock-crash");
    let ready = root.join("lock-holder-ready");
    drop(store);

    let mut child = Command::new(std::env::current_exe().expect("current test executable"))
        .arg("--exact")
        .arg("locking::lock_holder_subprocess")
        .arg("--nocapture")
        .env("DRIFTCTL_RUN_STORE_LOCK_CHILD", "1")
        .env("DRIFTCTL_RUN_STORE_STATE_ROOT", &root)
        .env("DRIFTCTL_RUN_STORE_REPOSITORY", &repository)
        .env("DRIFTCTL_RUN_STORE_READY", &ready)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("start lock holder subprocess");
    for _ in 0..100 {
        if ready.exists() {
            break;
        }
        thread::sleep(std::time::Duration::from_millis(10));
    }
    assert!(ready.exists(), "child acquired its lock before the timeout");
    assert_eq!(
        RunStore::open(&root, &repository, "run_01").expect_err("live holder is busy"),
        RunStoreError::Busy
    );

    child.kill().expect("kill lock holder without running Drop");
    child.wait().expect("reap lock holder");
    RunStore::open(&root, &repository, "run_01").expect("killed holder lock is released");

    fs::remove_dir_all(root).expect("remove isolated test directory");
}

#[test]
fn detached_snapshot_reader_waits_for_writer_release_instead_of_losing_its_wake() {
    use std::sync::mpsc;
    use std::time::Duration;
    let (root, repository, held, _) = create_store("snapshot-wait");
    let (started_tx, started_rx) = mpsc::channel();
    let (result_tx, result_rx) = mpsc::channel();
    let reader_root = root.clone();
    let reader = thread::spawn(move || {
        started_tx.send(()).unwrap();
        let store = RunStore::open_wait(&reader_root, &repository, "run_01").unwrap();
        result_tx
            .send(store.recover().unwrap().projection.goal.text)
            .unwrap();
    });
    started_rx.recv().unwrap();
    // Test probe only: not a deadline imposed on a native review or writer.
    assert!(result_rx.recv_timeout(Duration::from_millis(50)).is_err());
    drop(held);
    assert_eq!(
        result_rx.recv_timeout(Duration::from_secs(5)).unwrap(),
        "persist the run"
    );
    reader.join().unwrap();
    fs::remove_dir_all(root).unwrap();
}
