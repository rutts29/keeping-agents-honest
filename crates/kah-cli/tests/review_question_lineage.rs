use kah_cli::observer_review::{
    questions,
    state::{Purpose, Request},
};
use kah_observer::review_evidence::{Document, NativeRecord, Report};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};

fn fixture(label: &str) -> (PathBuf, Request, Report) {
    let root = std::env::temp_dir().join(format!(
        "kah-question-lineage-{label}-{}",
        std::process::id()
    ));
    let ledger = Document::new(
        "ledger",
        "ledger",
        "kah:ledger",
        "No format decision exists.",
    );
    let mut transcript = Document::new(
        "native",
        "worker_transcript",
        "native:worker",
        "Keep the original formatting or normalize it?",
    );
    transcript.native_records.push(NativeRecord {
        start_byte: 0,
        end_byte: transcript.text.len(),
        context: json!({"record_type":"agentMessage","record_id":"native-question"}),
    });
    let request = Request {
        id: "first".into(),
        worker: "worker".into(),
        purpose: Purpose::Checkpoint,
        turn: "t".into(),
        head: Some("sha".into()),
        intent: json!({"goal":"Build the formatter.","items":[]}),
        trigger: json!({}),
        documents: vec![ledger.clone(), transcript],
        created_ms: 0,
    };
    let report = serde_json::from_value(json!({"request_id":"first","findings":[],"decision_units":{
        "version":7,"requirements":[{"id":"format","name":"Format choice","dimension":"intent",
        "authority_items":["goal"],"blocker":"user_decision",
        "citations":[{"source":ledger.id,"digest":ledger.digest,"quote":ledger.text}],
        "components":[],"checks":[],"judgment":{"kind":"needs_user","reason":"ambiguous_instructions",
        "summary":"The format is undecided.","question":"Which format should apply?",
        "options":["Original","Normalized"],"evidence_citations":[0],
        "ledger_review":{"reasoning":"The ledger has no answer.","citations":[0]}}}]}})).unwrap();
    (root, request, report)
}

fn native_report(request: &Request, report: &Report, reference: &str) -> Report {
    let mut value = serde_json::to_value(report).unwrap();
    let units = &mut value["decision_units"];
    units["version"] = json!(8);
    let unit = &mut units["requirements"][0];
    unit["existing_question_id"] = reference.into();
    unit["worker_question"] = json!(1);
    unit["judgment"]["question"] = request.documents[1].text.clone().into();
    unit["judgment"]["evidence_citations"] = json!([0, 1]);
    unit["citations"]
        .as_array_mut()
        .unwrap()
        .push(json!({"source":"native",
        "digest":request.documents[1].digest,"quote":request.documents[1].text}));
    serde_json::from_value(value).unwrap()
}

fn first(root: &Path, request: &Request, report: &Report) -> Value {
    questions::store(root, request, report, &json!({"needs_user":["format"]}))
        .unwrap()
        .remove(0)
}

#[test]
fn native_quote_refines_one_active_question_and_retains_superseded_lineage() {
    let (root, mut request, report) = fixture("refine");
    let original = first(&root, &request, &report);
    let native = native_report(&request, &report, original["id"].as_str().unwrap());
    request.id = "native-refinement".into();
    let updated = first(&root, &request, &native);
    assert_eq!(updated["id"], original["id"]);
    assert_eq!(updated["question"], request.documents[1].text);
    assert_eq!(updated["question_origin"], "worker");
    assert_eq!(updated["lineage"][0]["question"], original["question"]);
    assert_eq!(updated["lineage"][0]["status"], "superseded");
    assert_eq!(updated["delivery"], "queued");
    assert_eq!(questions::pending(&root).unwrap().len(), 1);
    assert!(
        questions::store(&root, &request, &native, &json!({"needs_user":["format"]}))
            .unwrap()
            .is_empty()
    );
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn unrelated_resolved_and_cross_worker_references_are_rejected() {
    let (root, mut request, report) = fixture("invalid");
    let original = first(&root, &request, &report);
    let assessment = json!({"needs_user":["format"]});
    let wrong = native_report(&request, &report, "question-unknown");
    assert!(questions::store(&root, &request, &wrong, &assessment).is_err());
    let valid = native_report(&request, &report, original["id"].as_str().unwrap());
    let mut unlinked = serde_json::to_value(&valid).unwrap();
    unlinked["decision_units"]["requirements"][0]
        .as_object_mut()
        .unwrap()
        .remove("existing_question_id");
    let unlinked: Report = serde_json::from_value(unlinked).unwrap();
    assert!(
        questions::store(&root, &request, &unlinked, &assessment)
            .unwrap_err()
            .to_string()
            .contains("REVIEW_QUESTION_REFERENCE_REQUIRED")
    );
    request.worker = "another-worker".into();
    assert!(questions::store(&root, &request, &valid, &assessment).is_err());
    request.worker = "worker".into();
    let mut resolved = original.clone();
    resolved["status"] = "answered".into();
    kah_cli::observer_review::source_store::save(
        &root.join("questions"),
        &format!("{}.json", original["id"].as_str().unwrap()),
        &resolved,
    )
    .unwrap();
    assert!(questions::store(&root, &request, &valid, &assessment).is_err());
    std::fs::remove_dir_all(root).unwrap();
}
