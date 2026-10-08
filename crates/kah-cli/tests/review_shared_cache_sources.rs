use kah_observer::{
    decision_units::ReviewUnits,
    review_evidence::{Document, VerifiedSources},
};
use serde_json::json;
#[test]
fn immutable_requirement_validation_reuses_a_source_digest_without_changing_results() {
    let doc = Document::new(
        "plan",
        "plan",
        "docs/PLAN.md",
        "Whole authority sentence.\n".repeat(10000).as_str(),
    );
    let units:ReviewUnits = serde_json::from_value(json!({"version":12,"requirements":[{
        "id":"preserve","name":"Preserve behavior","dimension":"intent","authority_items":["goal"],
        "citations":[{"source":"plan","digest":doc.digest,"quote":"Whole authority sentence."}],
        "components":[],"checks":[],"judgment":{"kind":"semantic","decision":"satisfied","reasoning":"Exact authority","evidence_citations":[0]}}]})).unwrap();
    let docs = vec![doc];
    let sources = VerifiedSources::new(&docs);
    let baseline_start = std::time::Instant::now();
    let expected = units.verify(&docs, &["goal".into()]).unwrap();
    for _ in 1..20 {
        assert_eq!(units.verify(&docs, &["goal".into()]).unwrap(), expected);
    }
    let baseline = baseline_start.elapsed().as_micros();
    let shared_start = std::time::Instant::now();
    for _ in 0..20 {
        assert_eq!(
            units
                .verify_with_sources(&sources, &["goal".into()], &["goal".into()])
                .unwrap(),
            expected
        );
    }
    assert_eq!(sources.digest_checks(), 1);
    assert_eq!(sources.quote_checks(), 1);
    eprintln!(
        "SHARED_SOURCE_MEASUREMENT {}",
        json!({"source_bytes":docs[0].text.len(),"requirements":20,
        "baseline_micros":baseline,"shared_micros":shared_start.elapsed().as_micros(),"shared_digest_checks":sources.digest_checks(),
        "shared_quote_checks":sources.quote_checks(),"measurement_not_a_limit":true})
    );
    let mut corrupt = docs;
    corrupt[0].text.push_str("Changed bytes.");
    let fresh = VerifiedSources::new(&corrupt);
    assert!(
        units
            .verify_with_sources(&fresh, &["goal".into()], &["goal".into()])
            .unwrap_err()
            .contains("REVIEW_SOURCE_CHANGED")
    );
}
