use super::*;

#[cfg(unix)]
#[test]
fn refuses_symlinked_state_components() {
    use std::os::unix::fs::symlink;

    let state_root = temporary_directory("symlink");
    let repository = state_root.join("repository");
    fs::create_dir(&repository).expect("create repository");
    let outside = state_root.join("outside");
    fs::create_dir(&outside).expect("create outside directory");
    symlink(&outside, state_root.join("repositories")).expect("place symlinked state component");
    let initial_history = history();

    assert!(matches!(
        RunStore::create(
            &state_root,
            &repository,
            "run_01",
            &initial_history,
            &projection(&initial_history)
        ),
        Err(RunStoreError::SymlinkRefused { .. })
    ));
    fs::remove_dir_all(state_root).expect("remove isolated test directory");

    let root = temporary_directory("symlink-projection");
    let repository = root.join("repository");
    fs::create_dir(&repository).expect("create repository");
    let history = history();
    let store = RunStore::create(
        &root,
        &repository,
        "run_01",
        &history,
        &projection(&history),
    )
    .expect("create ordinary run");
    let outside = root.join("outside-projection.json");
    fs::write(&outside, "outside").expect("write outside projection");
    fs::remove_file(store.path().join("projection.json")).expect("remove stored projection");
    symlink(&outside, store.path().join("projection.json")).expect("link projection outside state");
    drop(store);

    assert!(matches!(
        RunStore::open(&root, &repository, "run_01"),
        Err(RunStoreError::SymlinkRefused { .. })
    ));

    fs::remove_dir_all(root).expect("remove isolated test directory");
}

#[cfg(unix)]
#[test]
fn refuses_dangling_symlinked_pending_delta() {
    use std::os::unix::fs::symlink;

    let (root, repository, store, _) = create_store("dangling-pending-symlink");
    let pending = store.path().join("pending.jsonl");
    fs::remove_file(&pending).expect("remove pending delta");
    symlink(root.join("missing-pending.jsonl"), &pending).expect("create dangling pending symlink");
    drop(store);

    assert!(matches!(
        RunStore::open(&root, &repository, "run_01"),
        Err(RunStoreError::SymlinkRefused { .. })
    ));

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
