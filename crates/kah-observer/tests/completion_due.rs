use kah_observer::{
    decision_units::{self, ReviewUnits},
    judge::{Answer, Judgment, Metrics},
    review_evidence::Document,
};
use serde_json::{Value, json};
use std::collections::BTreeMap;
fn receipt() -> Value {
    json!({"worker":"worker","request":"review-current","head":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","event_id":"event-gate","event_kind":"user_gate","phase":"phase_at_event","requires_claim_verification":false})
}
fn units(doc: &Document, receipt: Value) -> ReviewUnits {
    serde_json::from_value(json!({"version":7,"requirements":[{
        "id":"integration","name":"CSV pipeline integration","dimension":"integration","authority_items":["goal"],"blocker":null,
        "citations":[{"source":doc.id,"digest":doc.digest,"quote":doc.text}],"components":[],"checks":[],
        "applicability":{"due":true,"unconditional":false,"reasoning":"This phase is at its explicit recorded gate.","evidence_citations":[0],"controller_due":{"receipt_citation":0,"receipt":receipt}},
        "judgment":{"kind":"semantic","decision":"violated","reasoning":"The native pipeline test failed on the event SHA.","evidence_citations":[0]}
    }]})).unwrap()
}
fn doc(kind: &str) -> Document {
    Document::new("completion-due",kind,"kah:controller",&serde_json::to_string_pretty(&json!({"worker":"worker","request":"review-current","head":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","receipts":[receipt()]})).unwrap())
}
fn judgment() -> Judgment {
    Judgment {
        backend: "jev".into(),
        model: "jev-1.13.0".into(),
        answers: BTreeMap::from([(
            "requirement.integration".into(),
            Answer::Choice {
                choice: "supported".into(),
                confidence: 0.99,
                probabilities: BTreeMap::from([("supported".into(), 1.0)]),
            },
        )]),
        metrics: Metrics::default(),
        fallback_reason: None,
    }
}
#[test]
fn literal_current_gate_due_cannot_discard_a_confirmed_violation_for_uncertain_applicability() {
    let d = doc("controller_completion_due");
    let u = units(&d, receipt());
    u.verify(&[d], &["goal".into()]).unwrap();
    assert!(!decision_units::questions(&u).contains_key("applicability.integration"));
    assert_eq!(
        decision_units::assess(&u, &judgment()).violations,
        ["integration"]
    );
}
#[test]
fn worker_text_and_receipts_for_another_request_or_sha_cannot_establish_due() {
    let fake = doc("working_code");
    assert!(
        units(&fake, receipt())
            .verify(&[fake], &["goal".into()])
            .unwrap_err()
            .starts_with("REVIEW_COMPLETION_RECEIPT_SOURCE")
    );
    for key in ["worker", "request", "head", "event_id"] {
        let d = doc("controller_completion_due");
        let mut r = receipt();
        r[key] = json!("different");
        assert!(
            units(&d, r)
                .verify(&[d], &["goal".into()])
                .unwrap_err()
                .starts_with("REVIEW_COMPLETION_RECEIPT_BINDING")
        );
    }
}
#[test]
fn a_candidate_claim_still_needs_actual_claim_confirmation_before_literal_due_applies() {
    let mut r = receipt();
    r["event_kind"] = json!("native_phase_claim");
    r["requires_claim_verification"] = json!(true);
    let d=Document::new("completion-due","controller_completion_due","kah:controller",&serde_json::to_string(&json!({"worker":"worker","request":"review-current","head":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","receipts":[r]})).unwrap());
    let u = units(&d, r);
    u.verify(&[d], &["goal".into()]).unwrap();
    let mut j = judgment();
    assert!(decision_units::assess(&u, &j).violations.is_empty());
    j.answers
        .insert("claim_made".into(), Answer::Noul { noul: 0.99 });
    assert_eq!(decision_units::assess(&u, &j).violations, ["integration"]);
}

#[test]
fn persistent_confirmed_phase_timing_applies_only_to_its_completion_requirements() {
    let d = doc("controller_completion_due");
    let mut u = units(&d, receipt());
    let app = u.requirements[0].applicability.as_mut().unwrap();
    app.controller_due = None;
    app.phase_completion = Some(true);
    app.due = false;
    assert_eq!(
        decision_units::assess_with_phase_due(&u, &judgment(), true).violations,
        ["integration"]
    );
    u.requirements[0]
        .applicability
        .as_mut()
        .unwrap()
        .phase_completion = Some(false);
    let result = decision_units::assess_with_phase_due(&u, &judgment(), true);
    assert!(result.violations.is_empty());
    assert_eq!(result.needs_thinker, ["integration"]);
}
