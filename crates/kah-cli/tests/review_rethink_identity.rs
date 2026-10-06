use kah_cli::observer_review::{
    state::{Purpose, Request},
    uncertainty,
};
use kah_observer::review_evidence::{Document, Report};
use serde_json::json;
fn fixture() -> (Request, Report) {
    let doc = Document::new(
        "transcript",
        "worker_transcript",
        "native:worker",
        "Tests passed on the delivered commit.",
    );
    let request = Request {
        id: "review-first".into(),
        worker: "worker".into(),
        purpose: Purpose::Checkpoint,
        turn: "first".into(),
        head: Some("a".repeat(40)),
        intent: json!({"revision":1,"goal":"Deliver tested work."}),
        trigger: json!({}),
        documents: vec![doc.clone()],
        created_ms: 1,
    };
    let report = serde_json::from_value(json!({"request_id":request.id,"findings":[],"decision_units":{"version":9,"requirements":[{
        "id":"closure","name":"Delivered tests","dimension":"integration","authority_items":["goal"],"blocker":null,"components":[],"checks":[],
        "citations":[{"source":doc.id,"digest":doc.digest,"quote":doc.text}],
        "applicability":{"due":true,"unconditional":true,"reasoning":"Due.","evidence_citations":[0]},
        "judgment":{"kind":"semantic","decision":"satisfied","reasoning":"The native result supports completion.","evidence_citations":[0]}
    }]}})).unwrap();
    (request, report)
}
#[test]
fn unchanged_item_sha_and_checked_evidence_do_not_start_another_automatic_cycle() {
    let root = std::env::temp_dir().join(format!("kah-rethink-identity-{}", std::process::id()));
    let (request, report) = fixture();
    let assessment = json!({"needs_thinker":["closure"]});
    assert!(uncertainty::fresh_followup(&root, &request, &report, &assessment).unwrap());
    let mut next = request.clone();
    next.id = "review-next".into();
    next.turn = "next".into();
    next.created_ms = 2;
    next.documents[0] = Document::new(
        "transcript",
        "worker_transcript",
        "native:worker",
        "Tests passed on the delivered commit.\nAutomated acknowledgement.",
    );
    let mut raw = serde_json::to_value(&report).unwrap();
    raw["request_id"] = json!(next.id);
    raw["decision_units"]["requirements"][0]["citations"][0]["digest"] =
        json!(next.documents[0].digest);
    let next_report: Report = serde_json::from_value(raw).unwrap();
    assert!(!uncertainty::fresh_followup(&root, &next, &next_report, &assessment).unwrap());
    next.trigger = json!({"reconsideration":{"request":request.id}});
    assert!(uncertainty::fresh_followup(&root, &next, &next_report, &assessment).unwrap());
    next.trigger = json!({});
    next.head = Some("b".repeat(40));
    assert!(uncertainty::fresh_followup(&root, &next, &next_report, &assessment).unwrap());
    next.id = "review-metadata-only".into();
    next.intent["revision"] = json!(2);
    assert!(!uncertainty::fresh_followup(&root, &next, &next_report, &assessment).unwrap());
    next.intent["goal"] = json!("Deliver tested work with new authorization.");
    assert!(uncertainty::fresh_followup(&root, &next, &next_report, &assessment).unwrap());
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn retained_loop_case_uses_whole_original_quotes_and_reopens_on_new_evidence() {
    let captured: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/repeated-checkpoint-evidence.json")).unwrap();
    let (mut request, report) = fixture();
    request.head = captured["head"].as_str().map(str::to_owned);
    request.intent["goal"] = captured["authorityQuote"].clone();
    request.documents[0] = Document::new(
        "transcript",
        "worker_transcript",
        "native:worker",
        captured["completionQuote"].as_str().unwrap(),
    );
    let mut raw = serde_json::to_value(report).unwrap();
    raw["decision_units"]["requirements"][0]["citations"][0]["digest"] =
        json!(request.documents[0].digest);
    raw["decision_units"]["requirements"][0]["citations"][0]["quote"] =
        captured["completionQuote"].clone();
    let report: Report = serde_json::from_value(raw.clone()).unwrap();
    let assessment = json!({"needs_thinker":["closure"]});
    let root = std::env::temp_dir().join(format!("kah-retained-rethink-{}", std::process::id()));
    assert!(uncertainty::fresh_followup(&root, &request, &report, &assessment).unwrap());
    request.id = "review-later".into();
    assert!(!uncertainty::fresh_followup(&root, &request, &report, &assessment).unwrap());
    request.documents[0] = Document::new(
        "transcript",
        "worker_transcript",
        "native:worker",
        "An independent check now confirms the remaining obligation.",
    );
    raw["decision_units"]["requirements"][0]["citations"][0]["digest"] =
        json!(request.documents[0].digest);
    raw["decision_units"]["requirements"][0]["citations"][0]["quote"] =
        json!(request.documents[0].text);
    let report: Report = serde_json::from_value(raw).unwrap();
    assert!(uncertainty::fresh_followup(&root, &request, &report, &assessment).unwrap());
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn a_real_delivery_proof_reopens_a_prior_same_sha_observation() {
    let root = std::env::temp_dir().join(format!("kah-proof-rethink-{}", std::process::id()));
    let (request, report) = fixture();
    let assessment = json!({"needs_thinker":["closure"]});
    assert!(uncertainty::fresh_followup(&root, &request, &report, &assessment).unwrap());
    let mut proven = request.clone();
    proven.id = "review-delivery".into();
    let head = proven.head.clone().unwrap();
    proven.trigger = json!({"events":[{"kind":"run_completion","completion_key":{"commit":head},"verification":{"commit":head,"command":"check","success":true,"exit_code":0,"elapsed_ms":1,"output":"PASS","output_truncated":false}}]});
    assert!(uncertainty::fresh_followup(&root, &proven, &report, &assessment).unwrap());
    std::fs::remove_dir_all(root).unwrap();
}
