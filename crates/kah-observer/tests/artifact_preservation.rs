use kah_observer::{
    decision_units::{self, ReviewUnits, artifact::Receipt},
    judge::{Judgment, Metrics},
    review_evidence::Document,
};
use serde_json::json;

fn fixture(changed: bool) -> (ReviewUnits, Document) {
    let receipt = Receipt {
        path: "tests/roundtrip.cjs".into(),
        baseline_blob: "a".repeat(40),
        delivered_blob: Some(if changed { "b" } else { "a" }.repeat(40)),
        delivered_sha: "c".repeat(40),
    };
    let document = Document::new(
        "artifact",
        "controller_artifact_digests",
        "git:delivered:test",
        &serde_json::to_string(&receipt).unwrap(),
    );
    let units=serde_json::from_value(json!({"requirements":[{
        "id":"test-bytes","name":"Keep supplied test unchanged","dimension":"intent","authority_items":["goal"],
        "citations":[{"source":document.id,"digest":document.digest,"quote":document.text}],"components":[],"checks":[],
        "applicability":{"due":true,"unconditional":true,"reasoning":"The supplied test must remain unchanged.","evidence_citations":[0]},
        "judgment":{"kind":"artifact_preservation","fact":{"receipt_citation":0,"receipt":receipt},"evidence_citations":[0]}
    }]})).unwrap();
    (units, document)
}
fn judgment() -> Judgment {
    Judgment {
        backend: "not_requested".into(),
        model: "none".into(),
        answers: Default::default(),
        metrics: Metrics::default(),
        fallback_reason: None,
    }
}
#[test]
fn code_compares_exact_digests_without_asking_jev_to_infer_equality() {
    for changed in [false, true] {
        let (units, doc) = fixture(changed);
        units.verify(&[doc], &["goal".into()]).unwrap();
        let questions = decision_units::questions(&units);
        // Reviewer-approved v7 rule: unconditional prohibitions always apply.
        // Both timing and exact blob equality are known code facts here.
        assert!(questions.is_empty());
        assert_eq!(decision_units::assess(&units, &judgment()).passed, !changed);
    }
}
#[test]
fn worker_marker_text_cannot_supply_controller_digest_provenance() {
    let (units, mut doc) = fixture(false);
    doc.kind = "worker_transcript".into();
    assert!(
        units
            .verify(&[doc], &["goal".into()])
            .unwrap_err()
            .starts_with("REVIEW_ARTIFACT_PROVENANCE")
    );
}
#[test]
fn a_report_cannot_replace_a_controller_digest_with_a_matching_value() {
    let (mut units, doc) = fixture(true);
    if let decision_units::Thinking::ArtifactPreservation { fact, .. } =
        &mut units.requirements[0].judgment
    {
        fact.receipt.delivered_blob = Some(fact.receipt.baseline_blob.clone());
    }
    assert!(
        units
            .verify(&[doc], &["goal".into()])
            .unwrap_err()
            .starts_with("REVIEW_ARTIFACT_CHANGED")
    );
}
