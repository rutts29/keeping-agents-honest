//! ControllerReceipt must enter the full cited v12 validator.
use super::*;

#[path = "v12_controller_receipt_routes.rs"]
mod routes;
// One shared builder; each test module includes its own copy.
#[allow(clippy::duplicate_mod)]
#[path = "synthetic_gate.rs"]
mod synthetic;

fn fixture() -> (Request, Report, Value) {
    let root =
        std::env::temp_dir().join(format!("controller-head-{}", crate::relay_store::nonce()));
    let head = routes::repository(&root);
    std::fs::remove_dir_all(root).unwrap();
    let value = synthetic::controller("worker-gate", &head);
    let request: Request = serde_json::from_value(value["request"].clone()).unwrap();
    let mut report: Report = serde_json::from_value(value["report"].clone()).unwrap();
    report
        .verification_contracts
        .as_mut()
        .unwrap()
        .retain(|r| r.requirement_id == "req-commit-tag");
    let input = json!({"intent":request.intent,"event":{"head":request.head}});
    (request, report, input)
}

#[test]
fn a_controller_receipt_enters_full_requirement_coverage() {
    let (request, report, mut input) = fixture();
    let mut questions = Questions::new();
    prepare(&request, &report, &mut input, &mut questions).unwrap();
    assert!(
        questions.contains_key("coverage_decision.req-commit-tag"),
        "v12 must not silently omit a valid controller-receipt method"
    );
    let context = &input["verification_contracts"]["req-commit-tag"];
    assert!(
        context["decision"]["requirements"]["req-commit-tag"]["name"]
            .as_str()
            .unwrap()
            .contains("and the result is reported")
    );
    assert!(
        !context["source_manifest"]["checked_sources"]
            .as_object()
            .unwrap()
            .is_empty()
    );
}
