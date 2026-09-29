use kah_core::intent_history::{GoalRevision, History, SourceProvider, SourceRef, SourceRole};
use kah_core::projection::project;
use kah_core::run_store::RunStore;
use kah_observer::hook_event_store::{HookEventStore, HookEventStoreError};
use std::fs;

#[test]
fn replayed_hook_is_deduplicated_but_a_later_identical_user_prompt_is_not_lost() {
    let root = std::env::temp_dir().join(format!("kah-receipts-{}", std::process::id()));
    fs::create_dir(&root).unwrap();
    let history = History::new(GoalRevision::new(
        1,
        "fixture",
        vec![SourceRef::new(
            SourceProvider::Codex,
            "session",
            "record",
            SourceRole::User,
            "sha256:fixture",
        )],
    ))
    .unwrap();
    let projection = project(&history, 16384).unwrap();
    let run =
        RunStore::create(root.join("state"), &root, "fixture", &history, &projection).unwrap();
    let store = HookEventStore::open(&run).unwrap();
    let digest = HookEventStore::prompt_digest("Keep this constraint");
    store.record("turn-1:0", &digest, 1).unwrap();
    store.record("turn-1:0", &digest, 1).unwrap();
    assert!(store.claim_provider_record("provider-1", &digest).unwrap());
    assert!(store.claim_provider_record("provider-1", &digest).unwrap());
    assert!(!store.claim_provider_record("provider-2", &digest).unwrap());
    assert!(
        store
            .record("turn-1:0", &HookEventStore::prompt_digest("changed"), 2)
            .is_err()
    );
    store.record("turn-2:0", &digest, 2).unwrap();
    assert!(store.claim_provider_record("provider-2", &digest).unwrap());
    drop(run);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn hook_receipt_errors_have_stable_reason_and_stage() {
    let error = HookEventStoreError::Invalid("fixture");
    assert!(
        error
            .to_string()
            .starts_with("HOOK_RECEIPT_INVALID stage=hook_receipt:")
    );
}
