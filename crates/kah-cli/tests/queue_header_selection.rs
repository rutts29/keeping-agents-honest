use kah_cli::observer_review::{
    queue::{self, Wake},
    state::{Purpose, Store},
};
use serde_json::json;
#[test]
fn headers_sort_without_resolving_unselected_bodies_and_originals_remain() {
    let worker = format!("queue-header-{}", kah_cli::relay_store::nonce());
    let store = Store::open(&worker).unwrap();
    kah_core::run_store::ensure_private_directory(&store.directory.join("queued")).unwrap();
    let wake = Wake {
        purpose: Purpose::Preparation,
        turn: "original-turn".into(),
        trigger: json!({"preparation":{"purpose":"drift","state":{"$kah_source":"f".repeat(64),"encoding":"utf8"}}}),
    };
    kah_cli::unattended_report::save(
        &store.directory.join("queued"),
        "original.json",
        &serde_json::to_value(&wake).unwrap(),
    )
    .unwrap();
    let headers = queue::header_entries(&worker).unwrap();
    assert_eq!(headers.len(), 1);
    assert_eq!(headers[0].1.turn, "original-turn");
    assert_eq!(headers[0].1.trigger["preparation"]["purpose"], "drift");
    assert!(headers[0].1.trigger["preparation"].get("state").is_none());
    assert!(store.directory.join("queued/original.json").exists());
    assert!(
        queue::entries(&worker).is_err(),
        "body integrity is still checked when requested"
    );
    let path = store.directory.parent().unwrap().to_owned();
    drop(store);
    std::fs::remove_dir_all(path).unwrap();
}
