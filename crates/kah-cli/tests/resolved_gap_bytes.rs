use kah_cli::observer_review::{consume, source_store, state};
use serde_json::Value;
use std::{fs, os::unix::fs::PermissionsExt};

#[test]
fn successful_consume_archives_exact_gap_bytes_and_leaves_other_requests_alone() {
    let root = std::env::temp_dir().join(format!(
        "resolved-gap-bytes-{}",
        kah_cli::relay_store::nonce()
    ));
    kah_core::run_store::ensure_private_directory(&root.join("sources")).unwrap();
    let bytes=b"{ \n\t\"request\" : \"review-1-1\", \"error\" : \"STORE_BUSY stage=run_store: held writer\"\n}\n";
    fs::write(root.join("gap.json"), bytes).unwrap();
    fs::set_permissions(root.join("gap.json"), fs::Permissions::from_mode(0o600)).unwrap();
    let foreign = b"{\"request\":\"review-2-2\",\"error\":\"STORE_BUSY stage=run_store\"}";
    fs::write(root.join("delivered-gap.json"), foreign).unwrap();
    fs::set_permissions(
        root.join("delivered-gap.json"),
        fs::Permissions::from_mode(0o600),
    )
    .unwrap();
    consume::resolve_gaps(&root, "review-1-1").unwrap();
    assert!(!root.join("gap.json").exists());
    assert_eq!(fs::read(root.join("delivered-gap.json")).unwrap(), foreign);
    let file = fs::read_dir(root.join("resolved-gaps"))
        .unwrap()
        .map(|e| e.unwrap().path())
        .find(|p| p.extension().is_some_and(|v| v == "json"))
        .unwrap();
    let archived: Value = state::read(&file).unwrap().unwrap();
    let raw = archived["original_record"]
        .as_str()
        .expect("raw bytes must remain recoverable");
    assert_eq!(
        source_store::load_text(std::path::Path::new(raw))
            .unwrap()
            .as_bytes(),
        bytes
    );
    assert_eq!(archived["resolved_by"], "successful_consume");
    assert_eq!(archived["original"]["request"], "review-1-1");
    fs::remove_dir_all(root).unwrap();
}
