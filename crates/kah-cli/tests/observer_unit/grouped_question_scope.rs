use super::*;
#[test]
fn only_a_named_risk_on_the_same_worker_sha_and_revision_can_reference_the_group() {
    let request = Request {
        id: "review-later".into(),
        worker: "worker".into(),
        purpose: crate::observer_review::state::Purpose::Checkpoint,
        turn: "turn".into(),
        head: Some("a".repeat(40)),
        intent: json!({"revision":3}),
        trigger: json!({}),
        documents: vec![],
        created_ms: 1,
    };
    let q = json!({"kind":"checkpoint_approval","worker":"worker","requirement":"gate_approval","head":request.head,"intent_revision":3,"unverified_risks":[{"requirement_id":"scope-choice"}]});
    assert!(matches(&q, &request, "scope-choice"));
    let root = std::env::temp_dir().join(format!(
        "kah-grouped-lineage-{}",
        crate::relay_store::nonce()
    ));
    kah_core::run_store::ensure_private_directory(&root.join("questions")).unwrap();
    kah_core::run_store::ensure_private_directory(&root.join("sources")).unwrap();
    let mut active = q.clone();
    active["id"] = "question-group".into();
    active["status"] = "pending".into();
    crate::observer_review::source_store::save(
        &root.join("questions"),
        "question-group.json",
        &active,
    )
    .unwrap();
    let mut r: kah_observer::decision_units::Requirement = serde_json::from_value(json!({
        "id":"scope-choice","name":"Scope choice","dimension":"intent",
        "authority_items":["goal"],"citations":[],"components":[],"checks":[],
        "existing_question_id":"question-group",
        "judgment":{"kind":"semantic","decision":"satisfied","reasoning":"Scope choice","evidence_citations":[0]}
    })).unwrap();
    assert_eq!(
        super::super::lineage::target(&root, &request, &r).unwrap(),
        Some(active)
    );
    r.existing_question_id = None;
    assert!(super::super::lineage::require_new(&root, &request, &r).is_err());
    std::fs::remove_dir_all(root).unwrap();
    assert!(!matches(&q, &request, "other-item"));
    let mut bad = q.clone();
    bad["worker"] = "another-worker".into();
    assert!(!matches(&bad, &request, "scope-choice"));
    bad = q.clone();
    bad["head"] = "b".repeat(40).into();
    assert!(!matches(&bad, &request, "scope-choice"));
    bad = q;
    bad["intent_revision"] = 4.into();
    assert!(!matches(&bad, &request, "scope-choice"));
}
