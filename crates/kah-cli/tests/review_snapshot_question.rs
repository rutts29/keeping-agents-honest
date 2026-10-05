use kah_cli::observer_review::{
    state::{Purpose, Request, Store},
    workspace,
};
use kah_observer::review_evidence::{Document, NativeRecord};
use serde_json::json;

fn fixture() -> (std::path::PathBuf, Request, serde_json::Value) {
    let worker = format!("snapshot-question-unit-{}", std::process::id());
    let root = std::env::temp_dir().join(&worker);
    let input = root.join("inputs/review-snapshot");
    std::fs::create_dir_all(&input).unwrap();
    std::fs::create_dir_all(root.join("outbox")).unwrap();
    std::fs::create_dir_all(root.join("sources")).unwrap();
    let ledger = Document::new("ledger", "ledger", "kah:ledger", "No policy answer exists.");
    let mut native = Document::new(
        "native",
        "worker_transcript",
        "native:worker",
        "Keep original formatting or normalize it?",
    );
    native.native_records.push(NativeRecord {
        start_byte: 0,
        end_byte: native.text.len(),
        context: json!({"record_type":"agentMessage","record_id":"native-question"}),
    });
    let questions=Document::new("pending-questions","controller_questions","kah:pending-questions",
        &json!([{"id":"question-snapshot","worker":worker,"requirement":"format","status":"pending"}]).to_string());
    let request = Request {
        id: "review-snapshot".into(),
        worker: worker.clone(),
        purpose: Purpose::Drift,
        turn: "t".into(),
        head: None,
        intent: json!({"goal":"Build a formatter.","items":[]}),
        trigger: json!({}),
        documents: vec![ledger.clone(), native.clone(), questions],
        created_ms: 0,
    };
    let report = json!({"request_id":request.id,"findings":[{"topic":"intent","claim":"The formatting policy is undecided.",
        "citations":[{"source":ledger.id,"digest":ledger.digest,"quote":ledger.text}]}],"decision_units":{"version":8,"requirements":[{
        "id":"format","name":"Formatting policy","dimension":"intent","authority_items":["goal"],
        "blocker":"user_decision","worker_question":1,"existing_question_id":"question-snapshot",
        "citations":[{"source":ledger.id,"digest":ledger.digest,"quote":ledger.text},
                     {"source":native.id,"digest":native.digest,"quote":native.text}],"components":[],"checks":[],
        "judgment":{"kind":"needs_user","reason":"ambiguous_instructions","summary":"Policy is undecided.",
        "question":native.text,"options":["Original","Normalized"],"evidence_citations":[0,1],
        "ledger_review":{"reasoning":"No prior answer resolves this.","citations":[0]}}}]}});
    (root, request, report)
}

#[test]
fn native_report_uses_captured_question_refs_without_access_to_host_state() {
    let (root, request, report) = fixture();
    let request_path = root.join("inputs/review-snapshot/request.json");
    let report_path = root.join("report.json");
    std::fs::write(&request_path, serde_json::to_vec(&request).unwrap()).unwrap();
    std::fs::write(&report_path, serde_json::to_vec(&report).unwrap()).unwrap();
    let result = workspace::submit(&[
        "observe".into(),
        "report".into(),
        "--request".into(),
        request_path.to_str().unwrap().into(),
        "--json".into(),
        report_path.to_str().unwrap().into(),
    ]);
    let parsed = serde_json::from_value(report.clone()).unwrap();
    assert!(
        kah_cli::observer_review::decisions::verify(&request, &parsed).is_err(),
        "the host must still reject a reference absent from its current state"
    );
    for reference in [None, Some("question-unknown")] {
        let mut invalid = report.clone();
        invalid["decision_units"]["requirements"][0]["existing_question_id"] = json!(reference);
        std::fs::write(&report_path, serde_json::to_vec(&invalid).unwrap()).unwrap();
        let rejected = workspace::submit(&[
            "observe".into(),
            "report".into(),
            "--request".into(),
            request_path.to_str().unwrap().into(),
            "--json".into(),
            report_path.to_str().unwrap().into(),
        ]);
        assert!(
            rejected
                .unwrap_err()
                .to_string()
                .starts_with("REVIEW_QUESTION_REFERENCE")
        );
    }
    if let Some(parent) = Store::directory(&request.worker).unwrap().parent()
        && parent.exists()
    {
        std::fs::remove_dir_all(parent).unwrap();
    }
    std::fs::remove_dir_all(&root).unwrap();
    assert!(
        result.is_ok(),
        "valid snapshot reference required unavailable host state: {result:?}"
    );
}
