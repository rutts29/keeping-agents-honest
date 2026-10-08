use kah_cli::observer_review::source_store;
use serde_json::{Value, json};

#[test]
fn save_checks_every_original_byte_and_refuses_a_corrupt_reused_source() {
    let root = std::env::temp_dir().join(format!(
        "roundtrip-stream-{}",
        kah_cli::relay_store::nonce()
    ));
    kah_core::run_store::ensure_private_directory(&root).unwrap();
    source_store::initialize(&root, &root.join("workspace")).unwrap();
    let original = json!({"documents":[{"text":"Every whole user word 🦀.\n".repeat(8000),"native_records":[{"context":{"record_id":"native-one"}}]}],"trigger":{"preparation":{"state":{"intent":{"goal":"Do not lose any authority."},"values":[null,true,3,1.25]}}}});
    source_store::save(&root, "request.json", &original).unwrap();
    assert_eq!(
        source_store::read::<Value>(&root.join("request.json")).unwrap(),
        original
    );
    let recorded = std::fs::read(root.join("request.json")).unwrap();
    let encoded: Value = serde_json::from_slice(&recorded).unwrap();
    let digest = encoded["documents"][0]["$kah_source"].as_str().unwrap();
    std::fs::write(root.join("sources").join(digest), b"corrupt reused source").unwrap();
    assert!(source_store::save(&root, "request.json", &original).is_err());
    assert_eq!(std::fs::read(root.join("request.json")).unwrap(), recorded);
    std::fs::remove_dir_all(root).unwrap();
}
