use kah_cli::observer_review::{artifacts, source_store, state::Store};
use kah_observer::session::Binding;
use serde_json::json;
use std::fs;

fn binding(worker: &str) -> Binding {
    Binding {
        session: worker.into(),
        project: "/tmp/owned-fixture".into(),
        codex_home: Default::default(),
        native: Default::default(),
        model: "gpt-6.1-sol".into(),
        effort: "low".into(),
        timeout_secs: 90,
        judge: Default::default(),
        jev_model: "jev-latest".into(),
    }
}
#[test]
fn resume_never_fabricates_or_replaces_a_pre_work_baseline() {
    let worker = format!("artifact-resume-unit-{}", std::process::id());
    let binding = binding(&worker);
    artifacts::start(
        &binding,
        &json!({"hook_event_name":"SessionStart","source":"resume"}),
    )
    .unwrap();
    let path = Store::directory(&worker)
        .unwrap()
        .join("artifact-baseline/baseline.json");
    let first: serde_json::Value = source_store::read(&path).unwrap();
    assert_eq!(first["initial"], false);
    artifacts::start(
        &binding,
        &json!({"hook_event_name":"SessionStart","source":"startup"}),
    )
    .unwrap();
    assert_eq!(
        source_store::read::<serde_json::Value>(&path).unwrap(),
        first
    );
    fs::remove_dir_all(Store::directory(&worker).unwrap().parent().unwrap()).unwrap();
}
#[test]
fn delivered_digest_documents_are_bound_to_one_captured_sha() {
    let worker = format!("artifact-context-unit-{}", std::process::id());
    let directory = Store::directory(&worker).unwrap().join("artifact-baseline");
    let store = Store::delivery_at(&directory, false).unwrap();
    source_store::save(&directory,"baseline.json",&json!({"initial":true,"blobs":{"PLAN.md":"a".repeat(40),"tests.cjs":"b".repeat(40)},"unsupported":{}})).unwrap();
    let mut docs = vec![];
    artifacts::documents(
        &worker,
        &json!({"head":"c".repeat(40),"artifact_blobs":{"PLAN.md":"a".repeat(40)}}),
        &mut docs,
    )
    .unwrap();
    let receipts = docs
        .iter()
        .filter(|d| d.kind == "controller_artifact_digests")
        .map(|d| {
            serde_json::from_str::<kah_observer::decision_units::artifact::Receipt>(&d.text)
                .unwrap()
        })
        .collect::<Vec<_>>();
    assert_eq!(receipts.len(), 2);
    assert!(receipts.iter().all(|r| r.delivered_sha == "c".repeat(40)));
    assert!(
        receipts
            .iter()
            .find(|r| r.path == "tests.cjs")
            .unwrap()
            .delivered_blob
            .is_none()
    );
    assert_eq!(
        receipts
            .iter()
            .find(|r| r.path == "PLAN.md")
            .unwrap()
            .delivered_blob
            .as_deref(),
        Some("a".repeat(40).as_str())
    );
    drop(store);
    fs::remove_dir_all(Store::directory(&worker).unwrap().parent().unwrap()).unwrap();
}
