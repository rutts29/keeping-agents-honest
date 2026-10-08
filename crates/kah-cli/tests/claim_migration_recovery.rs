use kah_cli::observer_review::{queue::claim_migration, source_store, state::Store};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::fs;
fn setup() -> (Store, Value, Value) {
    let worker = format!("claim-migration-recovery-{}", kah_cli::relay_store::nonce());
    let store = Store::open(&worker).unwrap();
    for name in ["queued", "claim-migrations"] {
        kah_core::run_store::ensure_private_directory(&store.directory.join(name)).unwrap();
    }
    let original = json!({"purpose":"checkpoint","turn":"old","trigger":{"review_class":"gate","events":[{"kind":"native_phase_claim","question_set":2,"source":{"native_assistant_text":"Whole original words."}}],"user_authority_change":{"receipt":"whole-authority","text":"Never lose user words."}}});
    let mut converted = original.clone();
    converted["trigger"]["review_class"] = json!("commit");
    converted["trigger"]["events"][0]["question_set"] = json!(3);
    source_store::save(&store.directory.join("queued"), "old.json", &original).unwrap();
    let digest = format!(
        "{:x}",
        Sha256::digest(fs::read(store.directory.join("queued/old.json")).unwrap())
    );
    let record = json!({"state":"classified","original":"old.json","replacement":"new.json","digest":digest,"converted":converted,"authorization":"user-real","disposition":"covered_by_reclassification"});
    source_store::save(
        &store.directory.join("claim-migrations"),
        "case.json",
        &record,
    )
    .unwrap();
    (store, original, converted)
}
#[test]
fn a_crash_after_replacement_publish_recovers_once_and_keeps_the_original() {
    let (store, original, converted) = setup();
    source_store::save(&store.directory.join("queued"), "new.json", &converted).unwrap();
    claim_migration::recover(&store.directory).unwrap();
    claim_migration::recover(&store.directory).unwrap();
    assert_eq!(
        source_store::read::<Value>(&store.directory.join("queued/new.json")).unwrap(),
        converted
    );
    assert_eq!(
        source_store::read::<Value>(&store.directory.join("queue-retired/old.json")).unwrap(),
        original
    );
    assert!(!store.directory.join("queued/old.json").exists());
    let record =
        source_store::read::<Value>(&store.directory.join("claim-migrations/case.json")).unwrap();
    assert_eq!(record["state"], "complete");
    assert_eq!(record["replacement"], "new.json");
    let root = store.directory.parent().unwrap().to_owned();
    drop(store);
    fs::remove_dir_all(root).unwrap();
}
#[test]
fn a_changed_original_is_never_retired_or_replaced() {
    let (store, _, _) = setup();
    let path = store.directory.join("queued/old.json");
    fs::write(&path, b"changed original").unwrap();
    let error = claim_migration::recover(&store.directory)
        .unwrap_err()
        .to_string();
    assert!(error.starts_with("REVIEW_CLAIM_MIGRATION_SOURCE_CHANGED"));
    assert_eq!(fs::read(&path).unwrap(), b"changed original");
    assert!(!store.directory.join("queued/new.json").exists());
    let root = store.directory.parent().unwrap().to_owned();
    drop(store);
    fs::remove_dir_all(root).unwrap();
}
