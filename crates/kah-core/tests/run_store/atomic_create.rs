use super::*;

fn large_history() -> History {
    let mut history = history();
    for index in 0..6 {
        let record = pending_record(&history, &format!("i-{index}"));
        history.append(record.event).expect("extend history");
    }
    history
}

#[test]
fn crashing_creator_subprocess() {
    if std::env::var_os("KAH_RUN_STORE_CREATE_CHILD").is_none() {
        return;
    }
    let root = PathBuf::from(std::env::var_os("KAH_RUN_STORE_STATE_ROOT").expect("state root"));
    let repository =
        PathBuf::from(std::env::var_os("KAH_RUN_STORE_REPOSITORY").expect("repository"));
    let history = large_history();
    // The file-size limit set by the parent ends this process in the middle of
    // writing the run, as a crash or a full disk would.
    let _ = RunStore::create(
        &root,
        &repository,
        "run_01",
        &history,
        &projection(&history),
    );
}

#[test]
fn a_creator_killed_midway_leaves_the_run_id_available() {
    let root = temporary_directory("create-crash");
    let repository = root.join("repository");
    fs::create_dir(&repository).expect("create repository");
    let status = Command::new("sh")
        .arg("-c")
        .arg("ulimit -f 1; exec \"$@\"")
        .arg("sh")
        .arg(std::env::current_exe().expect("current test executable"))
        .arg("--exact")
        .arg("atomic_create::crashing_creator_subprocess")
        .arg("--nocapture")
        .env("KAH_RUN_STORE_CREATE_CHILD", "1")
        .env("KAH_RUN_STORE_STATE_ROOT", &root)
        .env("KAH_RUN_STORE_REPOSITORY", &repository)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .expect("run the creator under a file-size limit");
    assert!(!status.success(), "the creator must be interrupted");

    let history = history();
    let store = RunStore::create(
        &root,
        &repository,
        "run_01",
        &history,
        &projection(&history),
    )
    .expect("the same run id can be created after the interruption");
    drop(store);
    let reopened = RunStore::open(&root, &repository, "run_01").expect("open the created run");
    assert_eq!(
        reopened.recover().expect("recover").projection,
        projection(&history)
    );
    fs::remove_dir_all(root).expect("remove isolated test directory");
}

#[test]
fn an_existing_run_is_never_replaced_by_a_second_create() {
    let (root, repository, store, history) = create_store("create-twice");
    drop(store);
    let error = RunStore::create(
        &root,
        &repository,
        "run_01",
        &history,
        &projection(&history),
    )
    .expect_err("run exists");
    assert_eq!(error, RunStoreError::RunAlreadyExists);
    RunStore::open(&root, &repository, "run_01").expect("existing run is intact");
    fs::remove_dir_all(root).expect("remove isolated test directory");
}
