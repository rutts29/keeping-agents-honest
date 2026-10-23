//! Compact transport expands to the same Jev units as the full format.
use kah_cli::observer_review::{
    light_report,
    state::{Purpose, Request},
};
use serde_json::json;
fn request() -> Request {
    Request {
        id: "review-light".into(),
        worker: "w".into(),
        purpose: Purpose::Checkpoint,
        turn: "t".into(),
        head: None,
        intent: json!({}),
        trigger: json!({"review_class":"commit","decision_protocol_version":11}),
        documents: vec![],
        created_ms: 0,
    }
}
#[test]
fn compact_decisions_expand_without_inventing_a_verdict_or_losing_words() {
    let compact = json!({"request_id":"review-light","format":"commit-light-v2","decisions":[
        {"id":"privacy","name":"Keep notes private","dimension":"intent","authority_items":["goal"],
         "citations":[{"source":"ledger","digest":"sha256:x","quote":"Do not commit notes.","occurrence":0}],
         "decision":"violated","reasoning":"The exact native receipt shows notes committed.",
         "due":true,"unconditional":true,"obligation":"non_executable"}],"reminder":null});
    let expanded =
        light_report::decode(&request(), &serde_json::to_vec(&compact).unwrap()).unwrap();
    let v = serde_json::to_value(expanded).unwrap();
    assert_eq!(v["decision_units"]["version"], 11);
    let r = &v["decision_units"]["requirements"][0];
    assert_eq!(r["judgment"]["decision"], "violated");
    assert_eq!(
        r["judgment"]["reasoning"],
        compact["decisions"][0]["reasoning"]
    );
    assert_eq!(r["citations"], compact["decisions"][0]["citations"]);
    assert_eq!(r["applicability"]["unconditional"], true);
    assert_eq!(r["applicability"]["evidence_citations"], json!([0]));
    assert_eq!(r["judgment"]["evidence_citations"], json!([0]));
    assert_eq!(
        v["findings"][0]["claim"],
        compact["decisions"][0]["reasoning"]
    );
}
#[test]
fn compact_schema_is_not_available_to_a_gate_and_unknown_fields_are_rejected() {
    let mut req = request();
    req.trigger = json!({"review_class":"gate"});
    let report = br#"{"request_id":"review-light","format":"commit-light-v2","decisions":[]}"#;
    assert!(light_report::decode(&req, report).is_err());
    req = request();
    let bad=br#"{"request_id":"review-light","format":"commit-light-v2","decisions":[],"probability":0.99}"#;
    assert!(light_report::decode(&req, bad).is_err());
}
#[test]
fn schema_keeps_literal_and_question_escape_hatches_and_no_gate_contracts() {
    let schema = light_report::schema::schema();
    let properties = &schema["properties"]["decisions"]["items"]["properties"];
    assert!(properties["judgment"].is_object());
    assert!(properties["controller_trigger"].is_object());
    assert!(schema["properties"].get("verification_contracts").is_none());
    assert!(schema["properties"].get("findings").is_none());
    assert!(schema["properties"].get("decision_units").is_none());
    assert_eq!(
        schema["required"],
        json!(["request_id", "format", "decisions"])
    );
}
#[test]
fn compact_transport_does_not_change_the_generated_jev_questions() {
    let input = json!({"request_id":"review-light","format":"commit-light-v2","decisions":[
        {"id":"intent","name":"Follow assigned scope","dimension":"intent","authority_items":["goal"],
         "citations":[{"source":"ledger","digest":"d","quote":"Stay within scope."}],
         "decision":"satisfied","reasoning":"Observed actions follow the quoted scope.","due":true,
         "obligation":"non_executable"}]});
    let r = light_report::decode(&request(), &serde_json::to_vec(&input).unwrap()).unwrap();
    let units = r.decision_units.unwrap();
    let legacy: kah_observer::decision_units::ReviewUnits = serde_json::from_value(json!({
        "version":11,"requirements":[{"id":"intent","name":"Follow assigned scope","dimension":"intent",
        "authority_items":["goal"],"citations":[{"source":"ledger","digest":"d","quote":"Stay within scope."}],
        "components":[],"checks":[],"blocker":null,
        "obligation":{"kind":"non_executable","reasoning":"Observed actions follow the quoted scope.","evidence_citations":[0]},
        "applicability":{"due":true,"unconditional":false,"phase_completion":false,
        "reasoning":"Observed actions follow the quoted scope.","evidence_citations":[0]},
        "judgment":{"kind":"semantic","decision":"satisfied","reasoning":"Observed actions follow the quoted scope.","evidence_citations":[0]}}]})).unwrap();
    assert_eq!(
        serde_json::to_value(kah_observer::decision_units::questions(&units)).unwrap(),
        serde_json::to_value(kah_observer::decision_units::questions(&legacy)).unwrap()
    );
    assert_eq!(
        units.requirements[0].obligation.as_ref().unwrap().kind,
        kah_observer::decision_units::ObligationKind::NonExecutable
    );
}

#[test]
fn report_type_errors_name_the_field_the_observer_must_correct() {
    // A type error must name the field path the observer needs to correct.
    let mut req = request();
    req.trigger = json!({});
    let report = json!({"request_id":"review-light","findings":[{"topic":"intent",
        "claim":"c","citations":[{"source":"s","digest":"d","quote":"q","occurrence":{"index":1}}]}]});
    let error = light_report::decode(&req, &serde_json::to_vec(&report).unwrap())
        .unwrap_err()
        .to_string();
    assert!(
        error.starts_with("REVIEW_REPORT_JSON stage=report: "),
        "{error}"
    );
    assert!(
        error.contains("findings[0].citations[0].occurrence: invalid type: map, expected usize"),
        "{error}"
    );
}
