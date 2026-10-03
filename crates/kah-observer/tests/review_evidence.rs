use kah_observer::review_evidence::{Document, Report};
use serde_json::json;

#[test]
fn only_exact_source_bound_quotes_reach_the_judge() {
    let document = Document::new(
        "tool-1",
        "worker_transcript",
        "native:turn/tool-1",
        "Tests on commit abc failed.\nPhase 2 is not integrated.\n",
    );
    let value = json!({"request_id":"review-1","findings":[{"topic":"integration","claim":"Combined tests failed","citations":[{"source":"tool-1","digest":document.digest,"quote":"Tests on commit abc failed.","occurrence":0}]}]});
    let report: Report = serde_json::from_value(value.clone()).unwrap();
    let verified = report
        .verify("review-1", std::slice::from_ref(&document))
        .unwrap();
    assert_eq!(
        verified[0]["citations"][0]["quote"],
        "Tests on commit abc failed."
    );
    assert_eq!(verified[0]["citations"][0]["uri"], "native:turn/tool-1");
    let mut forged = value.clone();
    forged["findings"][0]["citations"][0]["quote"] = "All tests passed.".into();
    assert!(
        serde_json::from_value::<Report>(forged)
            .unwrap()
            .verify("review-1", std::slice::from_ref(&document))
            .is_err()
    );
    assert!(report.verify("different-request", &[document]).is_err());
}

#[test]
fn stale_digests_and_model_probabilities_are_not_evidence() {
    let document = Document::new("plan", "plan", "docs/PLAN.md", "Wire both phases now.");
    let value = json!({"request_id":"r","findings":[{"topic":"drift","claim":"Different work","citations":[{"source":"plan","digest":"stale","quote":"Wire both phases now."}]}]});
    assert!(
        serde_json::from_value::<Report>(value.clone())
            .unwrap()
            .verify("r", &[document])
            .is_err()
    );
    let mut probabilities = value;
    probabilities["confidence"] = json!(0.99);
    assert!(serde_json::from_value::<Report>(probabilities).is_err());
}

#[test]
fn duplicate_quotes_need_an_existing_occurrence_and_empty_reports_fail() {
    let document = Document::new(
        "transcript",
        "worker_transcript",
        "native:all",
        "first\nBLUE\nsecond\nBLUE\n",
    );
    let mut value = json!({"request_id":"r","findings":[{"topic":"drift","claim":"Later BLUE","citations":[{"source":"transcript","digest":document.digest,"quote":"BLUE","occurrence":1}]}]});
    let report: Report = serde_json::from_value(value.clone()).unwrap();
    assert_eq!(
        report.verify("r", std::slice::from_ref(&document)).unwrap()[0]["citations"][0]["line"],
        4
    );
    value["findings"][0]["citations"][0]["occurrence"] = 2.into();
    assert!(
        serde_json::from_value::<Report>(value)
            .unwrap()
            .verify("r", &[document])
            .is_err()
    );
    let empty: Report = serde_json::from_value(json!({"request_id":"r","findings":[]})).unwrap();
    assert!(empty.verify("r", &[]).is_err());
}
