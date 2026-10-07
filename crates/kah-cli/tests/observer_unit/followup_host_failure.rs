use super::*;

#[test]
fn host_check_error_finishes_the_job_without_repeating_unknown_checks() {
    let worker = format!("followup-host-failure-{}", std::process::id());
    let store = Store::open(&worker).unwrap();
    let state:super::super::super::state::State=serde_json::from_value(json!({"worker":{"provider":"codex","session":worker,"project":"/fixture","pane":null,"socket":null},"observer":{"session":null,"pane":null,"name":"fixture","workspace":"/fixture","model":"gpt-6.1-sol","effort":"low","transcript":null},"pending":null,"latest":null,"last_verified_turn":null,"checkpoint":null,"deferrals":[],"wakes":0,"native_tokens":0,"stopped":false})).unwrap();
    store.save(&state).unwrap();
    let root = store.directory.clone();
    drop(store);
    let path = root.join("gate-followups").join("followup-fixture");
    kah_core::run_store::ensure_private_directory(&path).unwrap();
    source_store::save(&path, "job.json", &json!({"retained":"original job"})).unwrap();
    failure(
        &root,
        &path,
        &worker,
        "REVIEW_FOLLOWUP_CHECK_UNKNOWN stage=verification",
    )
    .unwrap();
    assert!(jobs(&root).unwrap().is_empty());
    let saved: Value = source_store::read(&path.join("finished.json")).unwrap();
    assert_eq!(saved["status"], "host_failed");
    let diagnostic: Value = source_store::read(&path.join("failure.json")).unwrap();
    assert!(
        diagnostic["error"]
            .as_str()
            .unwrap()
            .contains("CHECK_UNKNOWN")
    );
    assert!(
        !super::super::super::reminder_queue::pending(&root)
            .unwrap()
            .is_empty()
    );
    assert!(path.join("job.json").exists());
    std::fs::remove_dir_all(root.parent().unwrap()).unwrap();
}

#[test]
fn waiting_builder_job_remains_kickable_after_stop_until_completion_is_durable() {
    let worker = format!("followup-late-marker-{}", std::process::id());
    let root = Store::directory(&worker).unwrap();
    let path = root.join("gate-followups").join("followup-late-marker");
    kah_core::run_store::ensure_private_directory(&root.join("sources")).unwrap();
    kah_core::run_store::ensure_private_directory(&path).unwrap();
    source_store::save(
        &path,
        "job.json",
        &json!({"waiting_for":"native turn completion"}),
    )
    .unwrap();
    // A Stop kick may see an incomplete native turn; it cannot consume the job.
    assert!(pending(&worker).unwrap());
    let driver = Store::delivery_at(&root.join("gate-followup-driver"), false).unwrap();
    assert!(pending(&worker).unwrap());
    drop(driver);
    // After the durable marker arrives, the existing idle controller's next
    // sample still sees pending work and can run the same job, not another ask.
    assert_eq!(jobs(&root).unwrap(), vec![path.clone()]);
    source_store::save(
        &path,
        "finished.json",
        &json!({"status":"awaiting_thinker"}),
    )
    .unwrap();
    assert!(!pending(&worker).unwrap());
    std::fs::remove_dir_all(root.parent().unwrap()).unwrap();
}

#[test]
fn transient_history_failure_keeps_the_canonical_job_recoverable() {
    let worker = format!("followup-transient-failure-{}", std::process::id());
    let store = Store::open(&worker).unwrap();
    let state:super::super::super::state::State=serde_json::from_value(json!({"worker":{"provider":"codex","session":worker,"project":"/fixture","pane":null,"socket":null},"observer":{"session":null,"pane":null,"name":"fixture","workspace":"/fixture","model":"gpt-6.1-sol","effort":"low","transcript":null},"pending":null,"latest":null,"last_verified_turn":null,"checkpoint":null,"deferrals":[],"wakes":0,"native_tokens":0,"stopped":false})).unwrap();
    store.save(&state).unwrap();
    let root = store.directory.clone();
    drop(store);
    let path = root.join("gate-followups/followup-transient");
    kah_core::run_store::ensure_private_directory(&path).unwrap();
    source_store::save(&path, "job.json", &json!({"retained":true})).unwrap();
    failure(
        &root,
        &path,
        &worker,
        "CODEX_CONNECTION stage=history: timed out",
    )
    .unwrap();
    assert_eq!(jobs(&root).unwrap(), vec![path.clone()]);
    assert!(!path.join("finished.json").exists());
    assert!(path.join("failure.json").exists());
    std::fs::remove_dir_all(root.parent().unwrap()).unwrap();
}
