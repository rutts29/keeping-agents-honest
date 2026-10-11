use super::super::followup::Stage;
use super::*;
use kah_observer::verification_contract::ReceiptFact;

#[test]
fn a_true_head_receipt_does_not_verify_an_uncovered_behavior_claim() {
    let root = std::env::temp_dir().join("kah-controller-fact-coverage");
    let head = "a".repeat(40);
    let request = Request {
        id: "review-coverage".into(),
        worker: "worker".into(),
        purpose: Purpose::Checkpoint,
        turn: "turn".into(),
        head: Some(head.clone()),
        intent: json!({"revision":1}),
        trigger: json!({}),
        documents: vec![],
        created_ms: 0,
    };
    let round = VerificationRound {
        id: "followup-coverage".into(),
        binding: Binding::from_request(&request).unwrap(),
        stage: Stage::Checks,
        claimed: true,
        evidence: None,
        builder: None,
        checks: None,
    };
    let state:crate::observer_review::state::State=serde_json::from_value(json!({
        "worker":{"provider":"codex","session":"worker","project":root,"pane":null,"socket":null},
        "observer":{"session":null,"pane":null,"name":"fixture","workspace":root,"model":"gpt-6.1-sol","effort":"low","transcript":null},
        "pending":null,"latest":null,"last_verified_turn":null,"checkpoint":null,"deferrals":[],"wakes":0,"native_tokens":0,"stopped":false
    })).unwrap();
    let item = Item {
        requirement_id: "normalize".into(),
        claim: "normalize computes the required rounding map".into(),
        evidence_citations: vec![],
        coverage_reasoning: "HEAD is a commit; that does not test normalize".into(),
        method: Method::ControllerReceipt {
            facts: vec![ReceiptFact::HeadSha],
        },
    };
    let proof = DeliveryProof {
        head,
        fresh_checkout: true,
        tests: vec![],
        artifacts: vec![],
        recorded_checks_absent: false,
    };
    let outcome = one(
        &root,
        &state,
        &round,
        &item,
        json!({"covered":false}),
        &proof,
        &request,
    )
    .unwrap();
    assert!(!outcome.verified);
    assert!(outcome.needs_check);
    assert!(!outcome.semantic_risk_admissible);
}
