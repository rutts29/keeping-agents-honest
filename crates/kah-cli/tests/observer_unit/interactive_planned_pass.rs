//! Joined GOOD path: synthetic successful check/coverage, no agent or Jev.
use super::*;
// One shared builder; each test module includes its own copy.
#[allow(clippy::duplicate_mod)]
#[path = "synthetic_gate.rs"]
mod synthetic;

#[test]
fn successful_v12_plan_replaces_empty_inventory_and_passes_without_a_question() {
    let value = synthetic::verifier("worker-gate");
    let mut request: Request = serde_json::from_value(value["request"].clone()).unwrap();
    let report: Report = serde_json::from_value(value["report"].clone()).unwrap();
    let mut proof: DeliveryProof = serde_json::from_value(value["proof"].clone()).unwrap();
    proof.tests.clear();
    proof.recorded_checks_absent = true;
    request.trigger["interactive_gate_verification"] = json!({"proof":proof});
    let mut assessed = json!({"passed":true,"state":"checkpoint_passed","violations":[],
        "needs_user":[],"needs_thinker":[],"unproven":[]});
    super::super::interactive::guard(&request, &mut assessed).unwrap();
    assert!(assessed["passed"].is_null());
    let result = planned_result(&request, &report);
    append_planned_checks(&mut proof, &[result]);
    assert!(proof.passes(request.head.as_deref().unwrap()));
    assert!(!proof.recorded_checks_absent);
    request.trigger["verification_followup"] = json!({"stage":"checks_complete","proof":proof});
    // Labelled $0 final coverage outcome. Selection/model quality is live proof.
    assessed["passed"] = true.into();
    assessed["state"] = "checkpoint_passed".into();
    super::super::interactive::guard(&request, &mut assessed).unwrap();
    assert_eq!(assessed["passed"], true);
    assert_eq!(assessed["state"], "checkpoint_passed");
    assert!(
        super::super::captured(&request)
            .unwrap()
            .unwrap()
            .passes(request.head.as_deref().unwrap())
    );
    let root = std::env::temp_dir().join(format!("v12-good-{}", crate::relay_store::nonce()));
    assert!(
        super::super::store(&root, &request, &report, &assessed, &proof, &json!({}))
            .unwrap()
            .is_none()
    );
    assert!(!root.join("questions").exists());
}

fn planned_result(
    request: &Request,
    report: &Report,
) -> super::super::contract_execution::ItemResult {
    let plan = report
        .verification_contracts
        .as_ref()
        .unwrap()
        .iter()
        .find(|i| i.requirement_id == "normalize-behavior")
        .unwrap();
    let kah_observer::verification_contract::Method::TargetedCheck { command } = &plan.method
    else {
        panic!("the fixture plan changed")
    };
    assert!(report.decision_units.as_ref().unwrap().version() >= 12);
    serde_json::from_value(json!({"binding":Binding::from_request(request).unwrap(),
        "requirement_id":plan.requirement_id,"claim":plan.claim,"plan_digest":"synthetic-control-only",
        "verified":true,"needs_check":false,"semantic_risk_admissible":false,"coverage":{"covered":true,"synthetic":true},
        "check":{"command":command,"commit":request.head,"exit_code":0,"success":true,"output":"SYNTHETIC_SUCCESSFUL_PLANNED_CHECK"},
        "tried":[],"evidence":[]})).unwrap()
}
