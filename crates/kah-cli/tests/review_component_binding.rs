//! C never treats the observer-writable public workspace as cache authority.
use kah_cli::observer_review::{
    component_map::{Map, producer},
    source_store,
    state::{Purpose, Request, Store},
};
use kah_observer::review_evidence::Report;
use serde_json::json;
fn setup() -> (Store, Request, Report, std::path::PathBuf) {
    let worker = format!("map-binding-{}", kah_cli::relay_store::nonce());
    let store = Store::open(&worker).unwrap();
    let request = Request {
        id: "review-map-binding".into(),
        worker,
        purpose: Purpose::Checkpoint,
        turn: "turn".into(),
        head: Some("a".repeat(40)),
        intent: json!({}),
        trigger: json!({"review_class":"commit"}),
        documents: vec![],
        created_ms: 1,
    };
    let report: Report = serde_json::from_value(json!({"request_id":request.id,"findings":[],
        "component_map":{"version":1,"components":[]}}))
    .unwrap();
    let workspace = store.directory.join("fixture-workspace");
    kah_core::run_store::ensure_private_directory(&workspace).unwrap();
    (store, request, report, workspace)
}
#[test]
fn checked_private_pointer_binds_collector_and_public_replacement_is_not_trusted() {
    let (store, request, report, workspace) = setup();
    assert!(
        producer::current(&store.directory, &request.worker)
            .unwrap()
            .is_none()
    );
    producer::record(&store, &request, &workspace, &report).unwrap();
    let stamp = producer::current(&store.directory, &request.worker)
        .unwrap()
        .unwrap();
    let map: Map = serde_json::from_value(report.component_map.clone().unwrap()).unwrap();
    producer::check_collected(
        &stamp,
        &json!({"component_impact":{"declaration_digest":map.digest().unwrap()}}),
    )
    .unwrap();
    assert!(
        producer::check_collected(
            &stamp,
            &json!({"component_impact":{"declaration_digest":"tampered"}})
        )
        .unwrap_err()
        .to_string()
        .contains("REVIEW_MAP_BINDING")
    );
    std::fs::write(
        producer::path(&workspace),
        "{\"version\":999,\"components\":[]}",
    )
    .unwrap();
    assert_eq!(
        producer::current(&store.directory, &request.worker)
            .unwrap()
            .unwrap(),
        stamp
    );
    let archive: serde_json::Value = source_store::read(
        &store
            .directory
            .join("component-maps/review-map-binding.json"),
    )
    .unwrap();
    assert_eq!(archive["map_digest"], map.digest().unwrap());
    let root = store.directory.parent().unwrap().to_owned();
    drop(store);
    std::fs::remove_dir_all(root).unwrap();
}
#[test]
fn later_checked_declaration_changes_pointer_without_losing_the_original() {
    let (store, mut request, report, workspace) = setup();
    producer::record(&store, &request, &workspace, &report).unwrap();
    request.id = "review-later-map-binding".into();
    let mut later = report;
    later.request_id = request.id.clone();
    producer::record(&store, &request, &workspace, &later).unwrap();
    assert_eq!(
        producer::current(&store.directory, &request.worker)
            .unwrap()
            .unwrap()["request"],
        request.id
    );
    assert!(
        store
            .directory
            .join("component-maps/review-map-binding.json")
            .exists()
    );
    assert!(
        producer::current(&store.directory, "different-worker")
            .unwrap_err()
            .to_string()
            .contains("REVIEW_MAP_BINDING")
    );
    let root = store.directory.parent().unwrap().to_owned();
    drop(store);
    std::fs::remove_dir_all(root).unwrap();
}
