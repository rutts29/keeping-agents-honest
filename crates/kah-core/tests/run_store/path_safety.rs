use super::*;

#[test]
fn creates_a_fresh_nested_state_root() {
    let outer = temporary_directory("fresh-state-root");
    let state_root = outer.join("missing/state/kah_core");
    let repository = outer.join("repository");
    fs::create_dir(&repository).expect("create repository");
    let history = history();

    let store = RunStore::create(
        &state_root,
        &repository,
        "run_01",
        &history,
        &projection(&history),
    )
    .expect("create run under a previously absent state root");

    assert!(store.path().is_dir());
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;

        for directory in [
            state_root.as_path(),
            store.path(),
            &store.path().join("history"),
        ] {
            assert_eq!(
                fs::metadata(directory).unwrap().permissions().mode() & 0o077,
                0,
                "private state directory: {}",
                directory.display()
            );
        }
        for file in [
            store.path().join("projection.json"),
            store.path().join("pending.jsonl"),
        ] {
            assert_eq!(
                fs::metadata(&file).unwrap().permissions().mode() & 0o077,
                0,
                "private state file: {}",
                file.display()
            );
        }
    }
    fs::remove_dir_all(outer).expect("remove isolated test directory");
}

#[test]
fn derives_a_stable_repository_digest_and_rejects_path_like_run_ids() {
    let root = temporary_directory("identifiers");
    let repository = root.join("repository");
    fs::create_dir(&repository).expect("create repository");

    let first = repository_id(&repository).expect("derive repository id");
    let second = repository_id(repository.join(".")).expect("canonical repository id");
    assert_eq!(first, second);
    assert_eq!(first.len(), 64);
    assert!(first.bytes().all(|byte| byte.is_ascii_hexdigit()));
    assert!(!first.contains("repository"));
    assert!(RunId::parse("../other").is_err());
    assert!(RunId::parse(".hidden").is_err());
    assert!(RunId::parse("run_01").is_ok());

    fs::remove_dir_all(root).expect("remove isolated test directory");
}

#[test]
fn creates_and_recovers_a_private_source_cursor_without_transcript_content() {
    let root = temporary_directory("source-cursor");
    let repository = root.join("repository");
    fs::create_dir(&repository).expect("create repository");
    let history = history();
    let cursor = source_cursor(&[("record-1", 'a'), ("record-2", 'b')]);
    let store = RunStore::create_with_source_cursor(
        &root,
        &repository,
        "run_01",
        &history,
        &projection(&history),
        &cursor,
    )
    .expect("create projection then source cursor");
    let source_path = store.path().join("source.json");
    let source = fs::read_to_string(&source_path).expect("read private source cursor");
    let source_json: serde_json::Value =
        serde_json::from_str(&source).expect("parse source cursor");
    assert_eq!(source_json["schema_version"], 1);
    assert_eq!(source_json["provider"], "bundle");
    assert_eq!(source_json["head"], "record-2");
    assert_eq!(
        source_json["accepted_records"].as_array().map(Vec::len),
        Some(2)
    );
    assert!(source.contains("private-session-locator"));
    assert!(source.contains("record-1"));
    assert!(source.contains(&digest('a')));
    assert!(!source.contains("raw transcript content must not persist"));
    assert!(!source.contains("\"content\""));
    let mut legacy_source = source_json.clone();
    for record in legacy_source["accepted_records"]
        .as_array_mut()
        .expect("legacy records")
    {
        record
            .as_object_mut()
            .expect("legacy record")
            .remove("role");
    }
    let legacy_cursor: SourceCursor =
        serde_json::from_value(legacy_source).expect("read pre-role source cursor");
    assert_eq!(legacy_cursor, cursor);
    legacy_cursor.validate().expect("validate pre-role cursor");
    #[cfg(unix)]
    assert_eq!(
        std::os::unix::fs::PermissionsExt::mode(
            &fs::metadata(&source_path)
                .expect("source metadata")
                .permissions()
        ) & 0o077,
        0,
        "source state must not be group/world-readable"
    );

    let recovered = store.recover().expect("recover source cursor");
    assert_eq!(recovered.source_cursor, Some(cursor));

    fs::remove_dir_all(root).expect("remove isolated test directory");
}

#[cfg(unix)]
#[test]
fn refuses_a_symlinked_private_source_cursor_and_only_updates_it_after_projection() {
    use std::os::unix::fs::symlink;

    let (root, _repository, store, mut history) = create_store("source-symlink");
    let pending = pending_record(&history, "i-1");
    history
        .append(pending.event.clone())
        .expect("extend history for projection commit");
    store
        .append_pending(pending)
        .expect("append pending record");
    let source_path = store.path().join("source.json");
    let outside = root.join("outside-source.json");
    fs::write(&outside, "outside").expect("write outside source fixture");
    symlink(&outside, &source_path).expect("link source cursor outside state");

    assert!(matches!(
        store.commit_projection_with_source_cursor(
            &projection(&history),
            &source_cursor(&[("record-1", 'a')])
        ),
        Err(RunStoreError::SymlinkRefused { .. })
    ));
    fs::remove_file(&source_path).expect("remove refused source symlink");
    assert_eq!(
        store
            .recover()
            .expect("projection committed before source write")
            .projection,
        projection(&history)
    );

    fs::remove_dir_all(root).expect("remove isolated test directory");
}

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
