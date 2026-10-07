use super::*;
use crate::observer_review::{
    failure::{Origin, correctable, origin, validation},
    state::Purpose,
};
use kah_observer::review_evidence::{Document, Report};
use serde_json::json;

fn fixture() -> (Request, Report) {
    let doc = Document::new("code", "committed_code", "git:HEAD", "Pipeline code.");
    let request = Request {
        id: "review-boundaries".into(),
        worker: "worker".into(),
        purpose: Purpose::Drift,
        turn: "turn".into(),
        head: Some("a".repeat(40)),
        intent: json!({"goal":"Verify pipeline","items":[]}),
        trigger: json!({}),
        documents: vec![doc.clone()],
        created_ms: 1,
    };
    let report = serde_json::from_value(json!({"request_id":request.id,"findings":[],"decision_units":{"version":11,"requirements":[{"id":"pipeline","name":"Pipeline composes modules","dimension":"integration","authority_items":["goal"],"citations":[{"source":doc.id,"digest":doc.digest,"quote":doc.text}],"components":[],"checks":[],"judgment":{"kind":"semantic","decision":"satisfied","reasoning":"The cited source implements composition","evidence_citations":[0]}}]}})).unwrap();
    (request, report)
}

#[test]
fn report_validation_walk_covers_bindings_obligations_contracts_manifests_and_dimensions() {
    let (request, report) = fixture();
    preflight_contexts(&request, &report, &[]).unwrap();
    for boundary in [
        "source",
        "obligation",
        "manifest",
        "contract",
        "controller",
        "coverage",
        "deferral",
    ] {
        let mut report = serde_json::to_value(&report).unwrap();
        let mut request = request.clone();
        let r = &mut report["decision_units"]["requirements"][0];
        match boundary {
            "source" => r["citations"][0]["source"] = "missing".into(),
            "obligation" => {
                r["obligation"] = json!({"kind":"non_executable","reasoning":"Historical","evidence_citations":[9]})
            }
            "manifest" => r["judgment"]["evidence_citations"] = json!([9]),
            "contract" => {
                report["verification_contracts"] = json!([{"requirement_id":"missing","claim":"Claim","coverage_reasoning":"Covered","evidence_citations":[0],"method":{"kind":"existing_check","test_index":0}}])
            }
            "controller" => {
                r["judgment"] = json!({"kind":"controller_fact","fact":{"worker":"foreign","request":request.id,"head":request.head,"receipt_citation":0,"property":{"property":"head_sha"}},"evidence_citations":[0]})
            }
            "coverage" => request.purpose = Purpose::Checkpoint,
            _ => {
                request.purpose = Purpose::Checkpoint;
                r["deferral_id"] = "unknown".into();
            }
        }
        let report: Report = serde_json::from_value(report).unwrap();
        let error = if matches!(boundary, "coverage" | "deferral") {
            checkpoint_coverage(&request, report.decision_units.as_ref().unwrap(), &[]).unwrap_err()
        } else {
            preflight_contexts(&request, &report, &[]).unwrap_err()
        };
        assert_boundary(error.as_ref(), boundary);
    }
    let error = validation(Origin::ReportValidation, || {
        validation(Origin::Host, || -> crate::Result<()> {
            Err("REVIEW_SOURCE_DIGEST stage=source_store".into())
        })
    })
    .unwrap_err();
    assert_eq!(origin(error.as_ref()), Origin::Host);
    assert!(!correctable(origin(error.as_ref()), &error.to_string()));
}
fn assert_boundary(error: &(dyn std::error::Error + 'static), boundary: &str) {
    assert_eq!(
        origin(error),
        Origin::ReportValidation,
        "{boundary}: {error}"
    );
    assert_eq!(
        correctable(origin(error), &error.to_string()),
        boundary != "controller",
        "{boundary}: {error}"
    );
}
