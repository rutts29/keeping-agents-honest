use super::super::followup::{Binding, Stage, VerificationRound};
use super::*;
#[test]
fn notice_failure_is_retained_then_recovers_once_without_wedging_the_review() {
    let root = std::env::temp_dir().join(format!(
        "kah-contract-delivery-{}",
        crate::relay_store::nonce()
    ));
    kah_core::run_store::ensure_private_directory(&root.join("sources")).unwrap();
    let request = Request {
        id: "review-delivery".into(),
        worker: "worker".into(),
        purpose: Purpose::Checkpoint,
        turn: "turn".into(),
        head: Some("a".repeat(40)),
        intent: json!({"revision":1}),
        trigger: json!({}),
        documents: vec![],
        created_ms: 1,
    };
    let round = VerificationRound {
        id: "followup-delivery".into(),
        binding: Binding::from_request(&request).unwrap(),
        stage: Stage::Checks,
        claimed: true,
        evidence: None,
        builder: None,
        checks: None,
    };
    let items = [json!({"id":"normalize","claim":"rounding export"})];
    let mut calls = 0;
    deliver_with(&root, &request, &round, &items, |_, _| {
        calls += 1;
        Err("fixture transport unavailable".into())
    })
    .unwrap();
    let path = root.join("gate-followups/followup-delivery/send-back/normalize.json");
    let failed: Value = super::super::super::source_store::read(&path).unwrap();
    assert_eq!(failed["submitted"], false);
    assert!(
        failed["error"]
            .as_str()
            .unwrap()
            .contains("transport unavailable")
    );
    deliver_with(&root, &request, &round, &items, |_, _| {
        calls += 1;
        Ok(())
    })
    .unwrap();
    deliver_with(&root, &request, &round, &items, |_, _| {
        calls += 1;
        Ok(())
    })
    .unwrap();
    assert_eq!(calls, 2);
    let recovered: Value = super::super::super::source_store::read(&path).unwrap();
    assert_eq!(recovered["submitted"], true);
    std::fs::remove_dir_all(root).unwrap();
}
