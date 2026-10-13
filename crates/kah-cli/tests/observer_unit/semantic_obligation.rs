//! Real VF16: suggested commands cannot replace a non-executable obligation.
use super::*;
use crate::observer_review::source_store;
use std::fs;
fn retained() -> (Request, Report, Value) {
    let value: Value = serde_json::from_str(include_str!(
        "../../../../docs/gates/evidence/phase3-obligation-routing/retained.json"
    ))
    .unwrap();
    (
        serde_json::from_value(value["request"].clone()).unwrap(),
        serde_json::from_value(value["report"].clone()).unwrap(),
        value["assessment"].clone(),
    )
}
#[test]
fn exact_method_conflict_stages_semantic_context_and_keeps_normalize_unconfirmed() {
    let (request, report, mut assessment) = retained();
    for id in ["req-intent", "req-drift", "req-plan", "req-preserve"] {
        assert!(
            semantic_resolution::semantic(&report, id),
            "linked non-executable {id}"
        );
    }
    assert!(!semantic_resolution::semantic(&report, "req-normalize"));
    let root = replay_root();
    assert!(
        semantic_resolution::guard_with(&root, &request, &report, &mut assessment, |_| Ok(()))
            .unwrap()
    );
    assert_eq!(assessment["semantic_resolution"]["status"], "pending");
    assert!(assessment["passed"].is_null());
    assert!(
        assessment["needs_thinker"]
            .as_array()
            .unwrap()
            .iter()
            .any(|id| id == "req-normalize")
    );
    assert!(assessment["needs_check"].as_array().unwrap().is_empty());
    assert!(finish_followup(&root, &request, &report, &assessment).unwrap());
    assert!(
        !root.join("gate-followups").exists(),
        "no send-back before resolution"
    );
    let saved: Request = source_store::read(&std::path::PathBuf::from(
        assessment["semantic_resolution"]["record"]
            .as_str()
            .unwrap(),
    ))
    .unwrap();
    assert!(saved.documents.iter().any(|d| d.id == "semantic-state"));
    let old_record = assessment["semantic_resolution"]["record"].clone();
    assert!(
        semantic_resolution::guard_with(&root, &request, &report, &mut assessment, |_| panic!(
            "one snapshot only"
        ))
        .unwrap()
    );
    assert_eq!(assessment["semantic_resolution"]["record"], old_record);
    fs::remove_dir_all(root).unwrap();
}

fn replay_root() -> std::path::PathBuf {
    let root = std::env::temp_dir().join(format!(
        "semantic-obligation-{}",
        crate::relay_store::nonce()
    ));
    kah_core::run_store::ensure_save_directory(&root.join("sources")).unwrap();
    let value: Value = serde_json::from_str(include_str!(
        "../../../../docs/gates/evidence/phase3-obligation-routing/retained.json"
    ))
    .unwrap();
    source_store::save(&root, "queue-order.json", &value["queue"]["order"]).unwrap();
    source_store::save(&root, "queue-metadata.json", &value["queue"]["metadata"]).unwrap();
    root
}
