use super::*;
use crate::observer_review::{
    gate_approval::followup::{Binding, Stage, VerificationRound},
    state::Purpose,
};
#[test]
fn an_accepted_ask_remains_a_fact_after_ledger_revision_changes() {
    let root = std::env::temp_dir().join(format!("kah-historical-ask-{}", std::process::id()));
    for path in [root.join("sources"), root.join("gate-followups")] {
        kah_core::run_store::ensure_private_directory(&path).unwrap();
    }
    let binding = Binding {
        worker: "worker".into(),
        review: "review-original".into(),
        head: "a".repeat(40),
        intent_revision: 2,
    };
    let round = VerificationRound {
        id: binding.id().unwrap(),
        binding: binding.clone(),
        stage: Stage::Evidence,
        claimed: false,
        evidence: None,
        builder: None,
        checks: None,
    };
    source_store::save(
        &root.join("gate-followups"),
        &format!("{}.json", round.id),
        &serde_json::to_value(&round).unwrap(),
    )
    .unwrap();
    let directory = root.join("gate-followups").join(&round.id);
    kah_core::run_store::ensure_private_directory(&directory).unwrap();
    let text = "Provide the recorded evidence for the same SHA.";
    source_store::save(
        &directory,
        "builder-request.json",
        &json!({"text":text,"receipt":round.builder_receipt()}),
    )
    .unwrap();
    let delivery = root.join("delivery");
    acknowledge(&delivery, &round, text);
    let mut request = Request {
        id: "review-current".into(),
        worker: "worker".into(),
        purpose: Purpose::Checkpoint,
        turn: "current".into(),
        head: Some(binding.head.clone()),
        intent: json!({"revision":3}),
        trigger: json!({}),
        documents: vec![],
        created_ms: 0,
    };
    add_at(&root, &mut request, &delivery).unwrap();
    let fact: Value = serde_json::from_str(&request.documents[0].text).unwrap();
    assert_eq!(fact["occurred"], true);
    assert_eq!(fact["request"], request.id);
    assert_eq!(fact["receipts"][0]["binding"]["intent_revision"], 2);
    request.head = Some("b".repeat(40));
    request.documents.clear();
    add_at(&root, &mut request, &delivery).unwrap();
    let fact: Value = serde_json::from_str(&request.documents[0].text).unwrap();
    assert_eq!(fact["occurred"], false);
    std::fs::remove_dir_all(root).unwrap();
}

fn acknowledge(delivery: &Path, round: &VerificationRound, text: &str) {
    let message = Message::new("worker", "kah-observer", &round.builder_receipt(), text).unwrap();
    let mut receipt = Receipt::open(delivery, &message).unwrap();
    receipt.save("accepted").unwrap();
}
