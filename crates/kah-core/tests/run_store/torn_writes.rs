use super::*;
use std::io::Write;
use std::os::unix::fs::PermissionsExt;

fn pending_bytes(store: &RunStore) -> Vec<u8> {
    fs::read(store.path().join("pending.jsonl")).expect("read pending delta")
}

fn append_raw(store: &RunStore, bytes: &[u8]) {
    let mut file = fs::OpenOptions::new()
        .append(true)
        .open(store.path().join("pending.jsonl"))
        .expect("open pending delta");
    file.write_all(bytes).expect("append raw bytes");
}

#[test]
fn a_torn_final_record_does_not_make_the_run_unreadable_or_poison_the_next_append() {
    for (case, tail) in [
        ("prefix", &b"{\"sequence\":3,\"ev"[..]),
        ("multibyte", &b"{\"text\":\"caf\xc3"[..]),
        ("unterminated", &b"{\"sequence\":3}"[..]),
    ] {
        let (root, repository, store, mut history) = create_store(&format!("torn-{case}"));
        let first = pending_record(&history, "i-1");
        history
            .append(first.event.clone())
            .expect("extend accepted history");
        store.append_pending(first.clone()).expect("append first");
        append_raw(&store, tail);
        drop(store);

        let reopened = RunStore::open(&root, &repository, "run_01").expect("reopen after tear");
        let recovered = reopened.recover().expect("recover after tear");
        assert_eq!(recovered.pending_delta, vec![first.clone()], "{case}");

        let second = pending_record(&history, "i-2");
        reopened
            .append_pending(second.clone())
            .expect("append after tear");
        assert!(pending_bytes(&reopened).ends_with(b"\n"), "{case}");
        drop(reopened);

        let reopened = RunStore::open(&root, &repository, "run_01").expect("reopen after repair");
        let recovered = reopened.recover().expect("recover after repair");
        assert_eq!(recovered.pending_delta, vec![first, second], "{case}");
        assert_eq!(
            pending_bytes(&reopened)
                .iter()
                .filter(|b| **b == b'\n')
                .count(),
            2
        );
        fs::remove_dir_all(root).expect("remove isolated test directory");
    }
}

#[test]
fn a_malformed_terminated_record_still_fails_closed() {
    let (root, repository, store, history) = create_store("torn-interior");
    store
        .append_pending(pending_record(&history, "i-1"))
        .expect("append first");
    append_raw(&store, b"not json\n");
    drop(store);

    let error = RunStore::open(&root, &repository, "run_01").expect_err("interior damage");
    assert!(
        matches!(error, RunStoreError::Serialization(_)),
        "{error:?}"
    );
    fs::remove_dir_all(root).expect("remove isolated test directory");
}

#[test]
fn a_projection_committed_before_its_rotation_survives_the_next_append() {
    let (root, repository, store, mut history) = create_store("rotation-then-append");
    let first = pending_record(&history, "i-1");
    history.append(first.event.clone()).expect("extend history");
    store.append_pending(first.clone()).expect("append first");
    fs::write(
        store.path().join("projection.json"),
        serde_json::to_vec(&projection(&history)).expect("serialize projection"),
    )
    .expect("simulate projection replaced before rotation");
    drop(store);

    let store = RunStore::open(&root, &repository, "run_01").expect("reopen interrupted run");
    let second = pending_record(&history, "i-2");
    store
        .append_pending(second.clone())
        .expect("append after crash");
    drop(store);

    let store = RunStore::open(&root, &repository, "run_01").expect("reopen after append");
    let recovered = store.recover().expect("recover after append");
    assert_eq!(recovered.pending_delta, vec![second.clone()]);
    history.append(second.event).expect("extend history again");
    store
        .commit_projection(&projection(&history))
        .expect("commit second projection");
    drop(store);

    let store = RunStore::open(&root, &repository, "run_01").expect("reopen after commit");
    let recovered = store.recover().expect("recover after commit");
    assert!(recovered.pending_delta.is_empty());
    assert_eq!(recovered.history.records().len(), 3);
    fs::remove_dir_all(root).expect("remove isolated test directory");
}

#[test]
fn a_recreated_pending_file_is_private() {
    let (root, repository, store, history) = create_store("pending-mode");
    fs::remove_file(store.path().join("pending.jsonl")).expect("model the crash after rotation");
    store
        .append_pending(pending_record(&history, "i-1"))
        .expect("append recreates the pending file");
    let mode = fs::metadata(store.path().join("pending.jsonl"))
        .expect("inspect pending file")
        .permissions()
        .mode();
    assert_eq!(mode & 0o777, 0o600);
    drop(store);
    RunStore::open(&root, &repository, "run_01").expect("reopen with private pending file");
    fs::remove_dir_all(root).expect("remove isolated test directory");
}

#[test]
fn a_rejected_supersession_edge_never_reaches_the_pending_file() {
    let (root, repository, store, history) = create_store("bad-edge");
    let mut record = pending_record(&history, "i-1");
    if let Event::RequirementAdded { item, .. } = &mut record.event {
        item.supersedes = vec![kah_core::intent_history::IntentId::new("missing")];
    }
    let before = pending_bytes(&store);
    store
        .append_pending(record)
        .expect_err("malformed edge is refused");
    assert_eq!(pending_bytes(&store), before);
    drop(store);
    RunStore::open(&root, &repository, "run_01").expect("the run still opens");
    fs::remove_dir_all(root).expect("remove isolated test directory");
}
