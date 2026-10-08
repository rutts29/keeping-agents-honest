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

#[test]
fn a_selected_preparation_batch_references_exact_original_files() {
    use kah_cli::observer_review::preparation::batch::{packet_refs, packets};
    use kah_cli::observer_review::state::Request;
    let worker = format!("queue-packet-ref-{}", kah_cli::relay_store::nonce());
    let store = Store::open(&worker).unwrap();
    kah_core::run_store::ensure_private_directory(&store.directory.join("queued")).unwrap();
    let original = Wake {
        purpose: Purpose::Preparation,
        turn: "original-turn".into(),
        trigger: json!({"preparation":{"purpose":"drift","questions":{},"state":{"intent":{"goal":"Whole original words","items":[["authority","Never commit notes."]]},"event":{"actions":[]}}}}),
    };
    let file = store.directory.join("queued/original.json");
    kah_cli::observer_review::source_store::save(
        file.parent().unwrap(),
        "original.json",
        &serde_json::to_value(&original).unwrap(),
    )
    .unwrap();
    let headers = queue::header_entries(&worker).unwrap();
    let (wake, paths) =
        packet_refs::group(&worker, &headers[0].1, std::slice::from_ref(&file)).unwrap();
    assert_eq!(paths, vec![file]);
    let packet = &wake.trigger["preparation_packets"][0];
    assert!(packet.get("trigger").is_none());
    let restored = packet_refs::original(packet).unwrap();
    assert_eq!(restored["trigger"], original.trigger);
    assert_eq!(restored["turn"], original.turn);
    let request = Request {
        id: "review-reference".into(),
        worker: worker.clone(),
        purpose: Purpose::Preparation,
        turn: "current-turn".into(),
        head: None,
        intent: json!({}),
        trigger: wake.trigger,
        documents: vec![],
        created_ms: 0,
    };
    let job =
        packets::job_for_packet(&request, 0, &request.trigger["preparation_packets"][0]).unwrap();
    assert_eq!(job.state["intent"]["goal"], "Whole original words");
    assert!(
        job.items
            .iter()
            .any(|i| i.active_user && i.pointer == "/intent/items/0")
    );
    let path = store.directory.parent().unwrap().to_owned();
    drop(store);
    std::fs::remove_dir_all(path).unwrap();
}
