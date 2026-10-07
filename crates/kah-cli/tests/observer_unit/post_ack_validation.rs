use super::*;
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
