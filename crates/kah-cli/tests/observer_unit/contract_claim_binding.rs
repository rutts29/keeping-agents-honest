use super::*;
use kah_observer::review_evidence::Document;

#[test]
fn coverage_uses_the_linked_requirement_not_a_relabelled_plan_claim() {
    let name = "normalize computes the required rounding map";
    let doc = Document::new("ledger", "ledger", "native:user", name);
    let request = Request {
        id: "review-claim".into(),
        worker: "worker".into(),
        purpose: Purpose::Checkpoint,
        turn: "turn".into(),
        head: Some("a".repeat(40)),
        intent: json!({"goal":name}),
        trigger: json!({}),
        documents: vec![doc.clone()],
        created_ms: 0,
    };
    let report: Report = serde_json::from_value(json!({
        "request_id":request.id,"findings":[],"decision_units":{"version":10,"requirements":[{
            "id":"normalize","name":name,"dimension":"integration","authority_items":["goal"],
            "citations":[{"source":doc.id,"digest":doc.digest,"quote":doc.text}],"components":[],"checks":[],
            "judgment":{"kind":"semantic","decision":"satisfied","reasoning":"Rounding behavior required.","evidence_citations":[0]}
        }]},"verification_contracts":[{
            "requirement_id":"normalize","claim":"HEAD is a commit","evidence_citations":[0],
            "coverage_reasoning":"HEAD alone cannot prove rounding.","method":{"kind":"controller_receipt","facts":[{"property":"head_sha"}]}
        }]
    })).unwrap();
    let mut input = json!({});
    let mut questions = Questions::new();
    prepare(&request, &report, &mut input, &mut questions).unwrap();
    let state = &input["verification_contracts"]["normalize"]["coverage"];
    assert_eq!(state["item"]["claim"], name);
    assert_eq!(state["proposed_claim"], "HEAD is a commit");
    assert!(questions.contains_key("coverage_subject.normalize"));
    assert!(questions.contains_key("coverage_assertion.normalize"));
    let linked = super::super::contract_execution::linked_item(
        &report,
        &report.verification_contracts.as_ref().unwrap()[0],
    )
    .unwrap();
    assert_eq!(linked.claim, name);
}
