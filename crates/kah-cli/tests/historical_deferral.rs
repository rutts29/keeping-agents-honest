use kah_cli::observer_review::{
    capture::historical,
    source_store,
    state::{Purpose, Request},
};
use kah_observer::review_evidence::{Citation, Document, Finding, Report, Topic};
use serde_json::json;

#[test]
fn original_ledger_anchor_survives_current_ledger_replacement_without_redefinition() {
    let root = std::env::temp_dir().join(format!("kah-history-{}", std::process::id()));
    std::fs::create_dir(&root).unwrap();
    source_store::initialize(&root, &root.join("workspace")).unwrap();
    std::fs::create_dir(root.join("requests")).unwrap();
    let original = Document::new(
        "ledger",
        "ledger",
        "kah:ledger",
        "Whole original requirement.\n",
    );
    source_store::save(
        &root.join("requests"),
        "review-original.json",
        &json!({"documents":[Document::new("ledger", "ledger", "kah:ledger", "Whole original requirement.\nEarlier context.\n")]}),
    )
    .unwrap();
    // Real incident: introduction id remains fixed, but the verified digest
    // was refreshed in a later review. CAS retains that exact whole source.
    source_store::save(
        &root.join("requests"),
        "review-later.json",
        &json!({"documents":[original]}),
    )
    .unwrap();
    let mut request = Request {
        id: "review-current".into(),
        worker: "worker".into(),
        purpose: Purpose::Checkpoint,
        turn: "turn".into(),
        head: None,
        intent: json!({"open_deferrals":[{"id":"stable","introduced_at":"review-original",
        "phase":"future phase","requirement":{"source":"ledger","uri":"kah:ledger","digest":original.digest,"quote":"Whole original requirement."}}]}),
        trigger: json!({}),
        documents: vec![Document::new(
            "ledger",
            "ledger",
            "kah:ledger",
            "Current changed authority.\n",
        )],
        created_ms: 0,
    };
    historical::add(&root, &mut request).unwrap();
    historical::add(&root, &mut request).unwrap();
    assert_eq!(request.documents.len(), 2);
    let anchor = &request.intent["open_deferrals"][0]["requirement"];
    let citation = Citation {
        source: anchor["source"].as_str().unwrap().into(),
        digest: original.digest.clone(),
        quote: anchor["quote"].as_str().unwrap().into(),
        occurrence: 0,
    };
    let verified = verified(&request, citation);
    assert_eq!(verified[0]["citations"][0]["uri"], "kah:ledger");
    assert_eq!(request.documents[0].text, "Current changed authority.\n");
    assert_eq!(request.documents[1].text, original.text);
    assert_eq!(anchor["quote"], "Whole original requirement.");
    assert_eq!(request.intent["open_deferrals"][0]["id"], "stable");
    std::fs::remove_dir_all(root).unwrap();
}

fn verified(request: &Request, citation: Citation) -> Vec<serde_json::Value> {
    let report = Report {
        verification_risks: None,
        verification_contracts: None,
        request_id: request.id.clone(),
        findings: vec![Finding {
            topic: Topic::Deferral,
            claim: "Historical declaration only.".into(),
            citations: vec![citation],
            deferral: None,
            checkpoint_scope: None,
        }],
        reminder: None,
        decision_units: None,
    };
    report.verify(&request.id, &request.documents).unwrap()
}
