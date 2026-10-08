use super::*;
fn report() -> Report {
    serde_json::from_value(json!({"request_id":"review-v12","findings":[],"decision_units":{"version":12,"requirements":[{
        "id":"normalize","name":"Normalize","dimension":"integration","authority_items":["goal"],"citations":[],"components":[],"checks":[],
        "judgment":{"kind":"semantic","decision":"satisfied","reasoning":"Implementation appears correct","evidence_citations":[0]}}]}})).unwrap()
}
fn result() -> ItemResult {
    serde_json::from_value(json!({"binding":{"worker":"worker","review":"review-v12","head":"a".repeat(40),"intent_revision":1},
        "requirement_id":"normalize","claim":"Normalize","plan_digest":"d","verified":false,"needs_check":true,
        "semantic_risk_admissible":false,"coverage":{"covered":false},"check":null,"tried":[],"evidence":[]})).unwrap()
}
#[test]
fn one_completed_round_routes_uncertain_coverage_but_never_a_failed_check() {
    let assessment = json!({"violations":[]});
    let mut item = result();
    terminal_coverage(&report(), &assessment, &mut item);
    assert!(item.semantic_risk_admissible);
    assert!(!item.needs_check);
    let mut failed = result();
    failed.check = Some(TestReceipt {
        command: "check".into(),
        commit: "a".repeat(40),
        exit_code: Some(1),
        success: false,
        output: "assertion failed".into(),
    });
    terminal_coverage(&report(), &assessment, &mut failed);
    assert!(!failed.semantic_risk_admissible);
    assert!(failed.needs_check);
}
#[test]
fn violations_and_legacy_protocol_cannot_enter_v12_risk_routing() {
    let mut item = result();
    terminal_coverage(&report(), &json!({"violations":["normalize"]}), &mut item);
    assert!(!item.semantic_risk_admissible);
    let mut legacy = report();
    legacy.decision_units.as_mut().unwrap().version = Some(11);
    terminal_coverage(&legacy, &json!({"violations":[]}), &mut item);
    assert!(!item.semantic_risk_admissible);
}

#[test]
fn proposed_target_executes_once_without_claiming_coverage_before_a_receipt() {
    let item = Item {
        requirement_id: "normalize".into(),
        claim: "Normalize".into(),
        evidence_citations: vec![],
        coverage_reasoning: "Checks normalization".into(),
        method: Method::TargetedCheck {
            command: "check".into(),
        },
    };
    let mut coverage = json!({"covered":false});
    proposed_target(&report(), &item, &mut coverage);
    assert_eq!(coverage["execute_reasoned_check"], true);
    assert_eq!(coverage["covered"], false);
    let mut legacy = report();
    legacy.decision_units.as_mut().unwrap().version = Some(11);
    let mut coverage = json!({"covered":false});
    proposed_target(&legacy, &item, &mut coverage);
    assert!(coverage["execute_reasoned_check"].is_null());
}
