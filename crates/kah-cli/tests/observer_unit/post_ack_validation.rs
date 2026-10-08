use super::*;
#[test]
fn an_old_supervised_child_cannot_publish_a_gap_on_a_new_request() {
    let worker = format!("bound-consume-{}", crate::relay_store::nonce());
    let store = Store::open(&worker).unwrap();
    let home = &store.directory;
    let state:super::super::state::State = serde_json::from_value(json!({"worker":{"provider":"codex","session":worker,"project":home,"pane":null,"socket":null},"observer":{"session":null,"pane":null,"name":"fixture","workspace":home,"model":"gpt-6.1-sol","effort":"low","transcript":null},"pending":{"id":"new","purpose":"drift","turn":"t","head":null,"created_ms":1,"native_tokens_before":null},"latest":null,"last_verified_turn":null,"checkpoint":null,"deferrals":[],"wakes":0,"native_tokens":0,"stopped":false})).unwrap();
    store.save(&state).unwrap();
    let current = json!({"request":"new","error":"REVIEW_SOURCE_DIGEST stage=source_store"});
    crate::unattended_report::save(home, "gap.json", &current).unwrap();
    let root = store.directory.clone();
    drop(store);
    let error = super::super::consume::supervision_failure::Failure {
        request: Some("old".into()),
        child_pid: 7,
        reason: "JEV_HTTP_STATUS stage=judge http_status=503".into(),
    };
    let captured = capture(&worker, &error).unwrap();
    assert_eq!(captured["request"], "old");
    let store = Store::open(&worker).unwrap();
    assert!(!gap::save(&store, &captured).unwrap());
    assert_eq!(
        crate::observer_review::state::read::<Value>(&root.join("gap.json"))
            .unwrap()
            .unwrap(),
        current
    );
    let archived = std::fs::read_dir(root.join("retired-gaps"))
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path();
    let record: Value = crate::observer_review::state::read(&archived)
        .unwrap()
        .unwrap();
    assert_eq!(record["gap"], captured);
    assert_eq!(record["current_request"], "new");
    assert!(
        gap::save(
            &store,
            &json!({"request":"new","error":"REVIEW_SOURCE_DIGEST stage=source_store"})
        )
        .unwrap()
    );
    drop(store);
    std::fs::remove_dir_all(root.parent().unwrap()).unwrap();
}
#[test]
fn unchanged_report_after_ack_waits_for_correction_instead_of_rejudgment() {
    let root = std::env::temp_dir().join(format!(
        "kah-post-ack-validation-{}",
        crate::relay_store::nonce()
    ));
    kah_core::run_store::ensure_private_directory(&root).unwrap();
    let report = root.join("submitted-review-one.json");
    std::fs::write(&report, b"{\"verification_risks\":[]}").unwrap();
    let request = super::super::state::Request {
        id: "review-one".into(),
        worker: "worker".into(),
        purpose: super::super::state::Purpose::Checkpoint,
        turn: "turn".into(),
        head: Some("a".repeat(40)),
        intent: json!({"revision":1}),
        trigger: json!({}),
        documents: vec![],
        created_ms: 1,
    };
    let gap = json!({"request":request.id,"origin":"report_validation","error":"REVIEW_FOLLOWUP_RISK_MISSING stage=verification","request_digest":digest(&serde_json::to_vec(&request).unwrap()),"report_digest":digest(&std::fs::read(&report).unwrap())});
    crate::unattended_report::save(&root, "gap.json", &gap).unwrap();
    assert!(unchanged(&root, &request, &report).unwrap());
    let mut later = request.clone();
    later.created_ms = 2;
    assert!(!unchanged(&root, &later, &report).unwrap());
    std::fs::write(&report, b"{\"verification_risks\":[\"corrected\"]}").unwrap();
    assert!(!unchanged(&root, &request, &report).unwrap());
    std::fs::write(&report, b"{\"verification_risks\":[]}").unwrap();
    let mut host = gap;
    host["origin"] = "host".into();
    crate::unattended_report::save(&root, "gap.json", &host).unwrap();
    assert!(!unchanged(&root, &request, &report).unwrap());
    std::fs::remove_dir_all(root).unwrap();
}
