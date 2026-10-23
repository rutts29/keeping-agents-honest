//! Observer declarations remain metadata, not user authority.
use kah_cli::observer_review::{
    component_map::producer,
    light_report, source_store,
    state::{Purpose, Request, Store},
};
use kah_observer::review_evidence::Report;
use serde_json::json;
fn request() -> Request {
    Request {
        id: "review-map-producer".into(),
        worker: format!("map-producer-{}", kah_cli::relay_store::nonce()),
        purpose: Purpose::Checkpoint,
        turn: "turn".into(),
        head: Some("a".repeat(40)),
        intent: json!({"goal":"Implement parser"}),
        trigger: json!({"review_class":"commit"}),
        documents: vec![],
        created_ms: 1,
    }
}
fn report(request: &Request) -> Report {
    let value = json!({"request_id":request.id,"format":"commit-light-v2","decisions":[{
        "id":"parser","name":"Parser works","dimension":"intent","authority_items":["goal"],
        "citations":[],"decision":"satisfied","reasoning":"Named behavior","due":true}],
        "component_map":{"version":1,"components":[{"name":"parser","path_globs":["src/**"],
        "requirements":["parser"],"tests":[{"name":"parser test","command":"cargo test parser"}],"e2e_flows":[]}]}});
    light_report::decode(request, &serde_json::to_vec(&value).unwrap()).unwrap()
}
#[test]
fn optional_map_survives_transport_and_only_bound_light_requirements_are_accepted() {
    let mut request = request();
    let mut report = report(&request);
    assert!(producer::validate(&request, &report).unwrap().is_some());
    report.component_map.as_mut().unwrap()["components"][0]["requirements"] = json!(["unknown"]);
    assert!(
        producer::validate(&request, &report)
            .err()
            .unwrap()
            .to_string()
            .contains("unknown")
    );
    request.trigger = json!({"review_class":"gate"});
    assert!(
        producer::validate(&request, &report)
            .err()
            .unwrap()
            .to_string()
            .contains("REVIEW_MAP_CLASS")
    );
    report.component_map = None;
    assert!(producer::validate(&request, &report).unwrap().is_none());
    assert!(
        !serde_json::to_value(report)
            .unwrap()
            .as_object()
            .unwrap()
            .contains_key("component_map")
    );
}
#[test]
fn checked_map_publication_preserves_private_request_sha_provenance_and_plain_native_input() {
    let request = request();
    let report = report(&request);
    let store = Store::open(&request.worker).unwrap();
    let workspace = store.directory.join("fixture-workspace");
    kah_core::run_store::ensure_private_directory(&workspace).unwrap();
    producer::record(&store, &request, &workspace, &report).unwrap();
    let public = producer::path(&workspace);
    let map: kah_cli::observer_review::component_map::Map =
        serde_json::from_slice(&std::fs::read(public).unwrap()).unwrap();
    map.validate(&["parser".into()]).unwrap();
    let private: serde_json::Value = source_store::read(
        &store
            .directory
            .join("component-maps")
            .join(format!("{}.json", request.id)),
    )
    .unwrap();
    assert_eq!(private["request"], request.id);
    assert_eq!(private["head"], request.head.unwrap());
    assert_eq!(private["worker"], request.worker);
    assert!(!store.directory.join("questions").exists());
    let root = store.directory.parent().unwrap().to_owned();
    drop(store);
    std::fs::remove_dir_all(root).unwrap();
}
