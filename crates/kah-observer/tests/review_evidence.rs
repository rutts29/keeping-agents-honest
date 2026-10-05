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
fn a_uri_citation_names_the_real_manifest_id_without_accepting_an_alias() {
    let document = Document::new(
        "plan-0",
        "plan",
        "PLAN.md",
        "Keep the supplied tests unchanged.",
    );
    let value = json!({"request_id":"r","findings":[{"topic":"intent","claim":"Preservation","citations":[{"source":"PLAN.md","digest":document.digest,"quote":document.text}]}]});
    let report: Report = serde_json::from_value(value).unwrap();
    let error = report.verify("r", &[document]).unwrap_err();
    assert!(error.starts_with("REVIEW_SOURCE_UNKNOWN stage=evidence"));
    assert!(error.contains("plan-0"));
    assert!(error.contains("document ids, not URIs"));
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

#[test]
fn brain_selected_whole_quotes_keep_full_source_pointers_without_document_copies() {
    let text = format!(
        "{}\nRELEVANT whole sentence.\n{}\nLate negation: not integrated.\n",
        "before 完整\n".repeat(1000),
        "after\n".repeat(1000)
    );
    let document = Document::new("plan", "plan", "docs/PLAN.md", &text);
    let findings = (0..30).map(|_|json!({"topic":"integration","claim":"Whole finding ".repeat(300),"citations":(0..9).map(|_|json!({"source":"plan","digest":document.digest,"quote":"RELEVANT whole sentence."})).collect::<Vec<_>>()})).collect::<Vec<_>>();
    let report: Report =
        serde_json::from_value(json!({"request_id":"large","findings":findings})).unwrap();
    let verified = report
        .verify("large", std::slice::from_ref(&document))
        .unwrap();
    assert_eq!(verified.len(), 30);
    assert_eq!(verified[29]["citations"].as_array().unwrap().len(), 9);
    let citation = &verified[29]["citations"][8];
    assert_eq!(citation["quote"], "RELEVANT whole sentence.");
    assert_eq!(citation["digest"], document.digest);
    assert_eq!(citation["uri"], "docs/PLAN.md");
    assert!(citation.get("context").is_none());
    let start = citation["start_byte"].as_u64().unwrap() as usize;
    let end = citation["end_byte"].as_u64().unwrap() as usize;
    assert_eq!(
        &document.text[start..end],
        citation["quote"].as_str().unwrap()
    );
    assert!(document.text.contains("Late negation: not integrated."));
}

#[test]
fn a_brain_span_cannot_begin_or_end_inside_a_word() {
    let document = Document::new(
        "message",
        "user_message",
        "native:message",
        "Do not delete confidential material.",
    );
    let value = json!({"request_id":"r","findings":[{"topic":"intent","claim":"Preserve confidential material","citations":[{"source":"message","digest":document.digest,"quote":"fidential material."}]}]});
    let report: Report = serde_json::from_value(value).unwrap();
    assert!(
        report
            .verify("r", std::slice::from_ref(&document))
            .unwrap_err()
            .starts_with("REVIEW_QUOTE_UNIT")
    );
}
