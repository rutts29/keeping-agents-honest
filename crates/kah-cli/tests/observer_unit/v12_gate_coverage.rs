use super::*;
use kah_observer::review_evidence::Document;
fn fixture() -> (Request, Report, Value) {
    let name = "normalize computes the required rounding map";
    let doc = Document::new("ledger", "ledger", "native:user", name);
    let head = "55911b4242f263a9d9531f08bb956a5e2f760d57";
    let request = Request {
        id: "review-v12".into(),
        worker: "worker".into(),
        purpose: Purpose::Checkpoint,
        turn: "t".into(),
        head: Some(head.into()),
        intent: json!({"goal":name,"items":[],"revision":1}),
        documents: vec![doc.clone()],
        created_ms: 0,
        trigger: json!({"events":[{"kind":"run_completion","completion_key":{"commit":head},"verification":{
            "command":"node verification.cjs","commit":head,"elapsed_ms":1,"exit_code":0,"output":"INDEPENDENT_PIPELINE_PASS\n","output_truncated":false,"success":true}}]}),
    };
    let report=serde_json::from_value(json!({"request_id":request.id,"findings":[],"decision_units":{"version":11,"requirements":[{
        "id":"normalize","name":name,"dimension":"integration","authority_items":["goal"],"citations":[{"source":doc.id,"digest":doc.digest,"quote":doc.text}],
        "components":[],"checks":[],"judgment":{"kind":"semantic","decision":"satisfied","reasoning":"Delivered rounding matches the user.","evidence_citations":[0]}}]},
        "verification_contracts":[{"requirement_id":"normalize","claim":name,"evidence_citations":[0],"coverage_reasoning":"The executed test asserts normalize then serialize for finite numbers, and fails if the delivered rounding map differs.","method":{"kind":"existing_check","test_index":0}}]})).unwrap();
    (
        request,
        report,
        json!({"intent":{"goal":name,"items":[]},"event":{"head":head}}),
    )
}
#[test]
fn gate_uses_reasoned_coverage_on_checked_same_sha_receipt() {
    let (request, report, mut input) = fixture();
    let mut questions = Questions::new();
    super::v12::prepare(
        &request,
        &report,
        &mut input,
        &mut questions,
        captured(&request).unwrap().as_ref(),
    )
    .unwrap();
    assert!(questions.contains_key("coverage_decision.normalize"));
    assert!(!questions.contains_key("coverage_subject.normalize"));
    assert!(!questions.contains_key("coverage_assertion.normalize"));
    let state = &input["verification_contracts"]["normalize"];
    assert_eq!(
        state["event"]["selected_host_check"]["commit"],
        request.head.as_ref().unwrap().as_str()
    );
    assert!(
        state["decision"]["requirements"]["normalize"]["judgment"]["reasoning"]
            .as_str()
            .unwrap()
            .contains("executed test")
    );
}
#[test]
fn coverage_requires_real_successful_same_sha_receipt() {
    let (mut request, report, mut input) = fixture();
    let mut questions = Questions::new();
    request.trigger["events"][0]["verification"]["commit"] = "a".repeat(40).into();
    assert!(prepare(&request, &report, &mut input, &mut questions).is_err());
}
#[test]
fn missing_method_does_not_become_a_coverage_claim() {
    let (request, mut report, mut input) = fixture();
    let mut questions = Questions::new();
    report.verification_contracts.as_mut().unwrap()[0].method = Method::NeedsCheck {
        why: "no known check".into(),
    };
    super::v12::prepare(
        &request,
        &report,
        &mut input,
        &mut questions,
        captured(&request).unwrap().as_ref(),
    )
    .unwrap();
    assert!(questions.is_empty());
}

#[test]
fn main_side_version_bump_enables_gate_dispatch() {
    let (request, mut report, mut input) = fixture();
    report.decision_units.as_mut().unwrap().version = Some(12);
    let mut questions = Questions::new();
    prepare(&request, &report, &mut input, &mut questions).unwrap();
    assert!(questions.contains_key("coverage_decision.normalize"));
}
#[test]
fn gate_report_errors_keep_report_origin_while_host_receipts_stay_host_failures() {
    use crate::observer_review::failure::{Origin, origin};
    let (request, mut report, mut input) = fixture();
    report.decision_units.as_mut().unwrap().version = Some(12);
    report.verification_contracts.as_mut().unwrap()[0].evidence_citations = vec![99];
    let error = prepare(&request, &report, &mut input, &mut Questions::new()).unwrap_err();
    assert_eq!(origin(error.as_ref()), Origin::ReportValidation);
    let (mut request, report, mut input) = fixture();
    let mut report = report;
    report.decision_units.as_mut().unwrap().version = Some(12);
    request.trigger["events"][0]["verification"]["commit"] = "a".repeat(40).into();
    let error = prepare(&request, &report, &mut input, &mut Questions::new()).unwrap_err();
    assert_eq!(origin(error.as_ref()), Origin::Host);
}
