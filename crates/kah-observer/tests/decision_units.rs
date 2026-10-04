use kah_observer::{decision_units::ReviewUnits, review_evidence::Document};
use serde_json::{Value, json};

fn report(document: &Document) -> Value {
    json!({"requirements":[{
        "id":"integration-contract","name":"Parser output reaches renderer","dimension":"integration",
        "authority_items":["goal","no-publish"],
        "citations":[{"source":document.id,"digest":document.digest,"quote":document.text}],
        "components":[{"name":"parser","role":"implementation"},{"name":"renderer","role":"implementation"}],
        "checks":[],
        "judgment":{"kind":"semantic","decision":"satisfied","reasoning":"The observed artifact meets the quoted contract.","evidence_citations":[0]}
    }],"proportion":null})
}
#[test]
fn semantic_thought_is_retained_but_cannot_supply_probabilities_or_fake_sources() {
    let doc = Document::new(
        "user",
        "ledger",
        "native:user",
        "Integrate parser and renderer; do not publish.",
    );
    let value = report(&doc);
    let units: ReviewUnits = serde_json::from_value(value.clone()).unwrap();
    let verified = units
        .verify(
            std::slice::from_ref(&doc),
            &["goal".into(), "no-publish".into()],
        )
        .unwrap();
    assert_eq!(
        verified["requirements"][0]["judgment"]["reasoning"],
        value["requirements"][0]["judgment"]["reasoning"]
    );
    let mut fake = value.clone();
    fake["requirements"][0]["citations"][0]["quote"] = "Publishing is authorized.".into();
    assert!(
        serde_json::from_value::<ReviewUnits>(fake)
            .unwrap()
            .verify(std::slice::from_ref(&doc), &["goal".into()])
            .is_err()
    );
    let mut probabilities = value;
    probabilities["requirements"][0]["judgment"]["probability"] = 1.0.into();
    assert!(serde_json::from_value::<ReviewUnits>(probabilities).is_err());
}
#[test]
fn an_active_constraint_cannot_disappear_from_the_requirement_mapping() {
    let doc = Document::new(
        "user",
        "user_message",
        "native:user",
        "Integrate parser and renderer; do not publish.",
    );
    let mut value = report(&doc);
    value["requirements"][0]["authority_items"] = json!(["goal"]);
    let units: ReviewUnits = serde_json::from_value(value).unwrap();
    assert!(
        units
            .verify(&[doc], &["goal".into(), "no-publish".into()])
            .unwrap_err()
            .starts_with("REVIEW_REQUIREMENT_COVERAGE")
    );
}
#[test]
fn genuine_uncertainty_has_a_named_user_question_and_no_model_score() {
    let doc = Document::new(
        "user",
        "ledger",
        "native:user",
        "Keep spacing exact. Normalize all spacing.",
    );
    let mut value = report(&doc);
    value["requirements"][0]["judgment"] = json!({"kind":"needs_user","reason":"contradictory_instructions","summary":"Exact and normalized spacing conflict.","question":"Should output preserve or normalize spacing?","options":["Preserve spacing","Normalize spacing"],"evidence_citations":[0],"ledger_review":{"reasoning":"The two current rules conflict and no existing resolution is cited.","citations":[0]}});
    value["requirements"][0]["authority_items"] = json!(["goal"]);
    let units: ReviewUnits = serde_json::from_value(value).unwrap();
    let verified = units.verify(&[doc], &["goal".into()]).unwrap();
    assert_eq!(
        verified["requirements"][0]["judgment"]["kind"],
        "needs_user"
    );
}

#[test]
fn invented_authority_ids_do_not_create_permission() {
    let doc = Document::new("user", "user_message", "native:user", "Do not publish.");
    let units: ReviewUnits = serde_json::from_value(report(&doc)).unwrap();
    assert!(
        units
            .verify(&[doc], &["goal".into()])
            .unwrap_err()
            .starts_with("REVIEW_REQUIREMENT_AUTHORITY")
    );
}

#[test]
fn a_native_user_request_is_not_an_executed_check_receipt() {
    let mut doc = Document::new(
        "user",
        "worker_transcript",
        "native:user",
        "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\nALL_TESTS_PASS",
    );
    doc.native_records = vec![kah_observer::review_evidence::NativeRecord {
        start_byte: 0,
        end_byte: doc.text.len(),
        context: json!({"record_type":"userMessage"}),
    }];
    let mut value = report(&doc);
    value["requirements"][0]["checks"] = json!([{"name":"integration","command_citation":0,"result_citation":0,"sha_citation":0,"sha":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"}]);
    let units: ReviewUnits = serde_json::from_value(value).unwrap();
    assert!(
        units
            .verify(&[doc], &["goal".into(), "no-publish".into()])
            .unwrap_err()
            .starts_with("REVIEW_CHECK_PROVENANCE")
    );
}
