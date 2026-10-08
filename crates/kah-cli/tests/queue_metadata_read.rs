use kah_cli::observer_review::{
    queue::{self, Wake},
    state::{Purpose, Store},
};
use serde_json::json;
#[test]
fn queue_age_reads_only_repair_link_metadata_not_the_original_payload() {
    let worker = format!("queue-age-fields-{}", std::process::id());
    let store = Store::open(&worker).unwrap();
    for name in ["queued", "queue-repairs", "queue-retired"] {
        kah_core::run_store::ensure_private_directory(&store.directory.join(name)).unwrap();
    }
    std::fs::write(store.directory.join("queue-retired/original.json"), b"{}").unwrap();
    let wake = Wake {
        purpose: Purpose::Drift,
        turn: "turn".into(),
        trigger: json!({"actions":[]}),
    };
    kah_cli::observer_review::source_store::save(
        &store.directory.join("queued"),
        "pending.json",
        &serde_json::to_value(&wake).unwrap(),
    )
    .unwrap();
    // A reader of link metadata must not resolve unrelated archived evidence.
    // Full source readers still detect this deliberately absent test payload.
    let archive = json!({"rows":[{"replacement":"pending.json","original":"original.json","content":{"$kah_source":"f".repeat(64),"encoding":"utf8"}}]});
    kah_cli::unattended_report::save(
        &store.directory.join("queue-repairs"),
        "links.json",
        &archive,
    )
    .unwrap();
    assert_eq!(queue::entries(&worker).unwrap().len(), 1);
    assert!(
        kah_cli::observer_review::source_store::read::<serde_json::Value>(
            &store.directory.join("queue-repairs/links.json")
        )
        .is_err()
    );
    let path = store.directory.parent().unwrap().to_owned();
    drop(store);
    std::fs::remove_dir_all(path).unwrap();
}
