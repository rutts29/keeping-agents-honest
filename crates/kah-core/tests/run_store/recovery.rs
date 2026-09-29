use super::*;

#[test]
fn creates_and_reopens_an_xdg_local_run_without_storing_the_repository_path() {
    let (root, repository, store, history) = create_store("create-reopen");
    let run_path = store.path().to_path_buf();
    assert!(run_path.starts_with(root.join("repositories")));
    assert!(run_path.join("history").is_dir());
    assert!(run_path.join("projection.json").is_file());
    assert!(run_path.join("pending.jsonl").is_file());
    assert!(!run_path.to_string_lossy().contains("repository"));
    drop(store);

    let reopened = RunStore::open(&root, &repository, "run_01").expect("reopen run store");
    let recovered = reopened.recover().expect("recover committed run");
    assert_eq!(recovered.projection, projection(&history));
    assert!(recovered.pending_delta.is_empty());

    fs::remove_dir_all(root).expect("remove isolated test directory");
}

#[test]
fn recovers_committed_projection_with_an_unrotated_pending_delta_and_ignores_temp_files() {
    let (root, repository, store, history) = create_store("recovery");
    let record = pending_record(&history, "i-1");
    store
        .append_pending(record.clone())
        .expect("append pending record");
    fs::write(
        store.path().join(".projection.json.tmp-interrupted"),
        "not a projection",
    )
    .expect("leave interrupted temp artifact");
    drop(store);

    let reopened = RunStore::open(&root, &repository, "run_01").expect("reopen run store");
    let recovered = reopened.recover().expect("recover after interruption");
    assert_eq!(recovered.projection, projection(&history));
    assert_eq!(recovered.pending_delta, vec![record]);

    fs::remove_dir_all(root).expect("remove isolated test directory");
}

#[test]
fn recovers_a_durable_projection_when_its_accepted_delta_was_not_yet_rotated() {
    let (root, repository, store, mut history) = create_store("projection-before-rotation");
    let record = pending_record(&history, "i-1");
    history
        .append(record.event.clone())
        .expect("extend accepted history");
    let accepted = projection(&history);
    store
        .append_pending(record.clone())
        .expect("append pending record");
    fs::write(
        store.path().join("projection.json"),
        serde_json::to_vec(&accepted).expect("serialize durable projection"),
    )
    .expect("simulate completed projection replacement before rotation");
    drop(store);

    let reopened = RunStore::open(&root, &repository, "run_01").expect("reopen interrupted run");
    let recovered = reopened.recover().expect("recover interrupted commit");
    assert_eq!(recovered.projection, accepted);
    assert_eq!(recovered.pending_delta, vec![record]);

    fs::remove_dir_all(root).expect("remove isolated test directory");
}

#[test]
fn atomically_replaces_projection_then_rotates_exact_pending_delta_to_immutable_history() {
    let (root, _repository, store, mut history) = create_store("commit-rotate");
    let record = pending_record(&history, "i-1");
    history
        .append(record.event.clone())
        .expect("extend accepted history");
    store
        .append_pending(record.clone())
        .expect("append pending record");

    let accepted = projection(&history);
    store
        .commit_projection(&accepted)
        .expect("commit projection");

    let recovered = store.recover().expect("recover committed projection");
    assert_eq!(recovered.projection, accepted);
    assert!(recovered.pending_delta.is_empty());
    let segments = fs::read_dir(store.path().join("history"))
        .expect("read immutable history")
        .map(|entry| entry.expect("history entry").path())
        .collect::<Vec<_>>();
    assert_eq!(segments.len(), 2);
    let rotated = segments
        .iter()
        .find(|path| {
            fs::read_to_string(path)
                .expect("read segment")
                .contains("i-1")
        })
        .expect("pending delta rotated to a segment");
    assert_eq!(
        fs::read_to_string(rotated).expect("read rotated delta"),
        serde_json::to_string(&record).expect("serialize record") + "\n"
    );

    fs::remove_dir_all(root).expect("remove isolated test directory");
}

#[test]
fn refuses_to_overwrite_an_immutable_history_segment() {
    let (root, _repository, store, mut history) = create_store("immutable-history");
    let record = pending_record(&history, "i-1");
    history
        .append(record.event.clone())
        .expect("extend accepted history");
    store.append_pending(record).expect("append pending record");
    let existing_segment = store
        .path()
        .join("history/00000000000000000002-00000000000000000002.jsonl");
    fs::write(&existing_segment, "immutable fixture\n").expect("create conflicting segment");

    assert!(matches!(
        store.commit_projection(&projection(&history)),
        Err(RunStoreError::ImmutableHistorySegment { .. })
    ));
    assert_eq!(
        fs::read_to_string(existing_segment).expect("read immutable fixture"),
        "immutable fixture\n"
    );

    fs::remove_dir_all(root).expect("remove isolated test directory");
}

#[test]
fn leaves_the_v1_ledger_in_place_without_migrating_it() {
    let root = temporary_directory("v1-coexistence");
    let repository = root.join("repository");
    let legacy = repository.join(".kah_core/ledger.jsonl");
    fs::create_dir_all(legacy.parent().expect("legacy parent")).expect("create legacy directory");
    fs::write(&legacy, "v1 ledger remains untouched\n").expect("write v1 ledger");
    let history = history();

    let store = RunStore::create(
        &root,
        &repository,
        "run_01",
        &history,
        &projection(&history),
    )
    .expect("create v2 run without migration");
    assert!(store.path().starts_with(root.join("repositories")));
    assert_eq!(
        fs::read_to_string(&legacy).expect("read v1 ledger"),
        "v1 ledger remains untouched\n"
    );

    fs::remove_dir_all(root).expect("remove isolated test directory");
}
