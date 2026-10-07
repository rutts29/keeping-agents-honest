use kah_observer::{
    decision_units::{ReviewUnits, assess},
    judge::{Answer, Judgment, Metrics},
    review_evidence::Document,
};
use serde_json::json;
use std::collections::BTreeMap;
fn units(kind: Option<&str>) -> ReviewUnits {
    let doc = Document::new("ledger", "ledger", "native:user", "Do not push or deploy.");
    let mut value = json!({"version":11,"requirements":[{"id":"no-push","name":"No push","dimension":"intent","authority_items":["goal"],"citations":[{"source":doc.id,"digest":doc.digest,"quote":doc.text}],"components":[],"checks":[],"applicability":{"due":true,"unconditional":true,"phase_completion":false,"reasoning":"Always applies","evidence_citations":[0]},"judgment":{"kind":"semantic","decision":"satisfied","reasoning":"Native history has no push/deploy","evidence_citations":[0]}}]});
    if let Some(kind) = kind {
        value["requirements"][0]["obligation"] = json!({"kind":kind,"reasoning":"This is observed historical compliance, not a program contract","evidence_citations":[0]});
    }
    serde_json::from_value(value).unwrap()
}
fn judgment(confidence: f64) -> Judgment {
    Judgment {
        backend: "jev".into(),
        model: "jev-latest".into(),
        answers: BTreeMap::from([(
            "requirement.no-push".into(),
            Answer::Choice {
                choice: "supported".into(),
                confidence,
                probabilities: BTreeMap::from([("supported".into(), 0.99)]),
            },
        )]),
        metrics: Metrics::default(),
        fallback_reason: None,
    }
}
#[test]
fn only_confirmed_explicit_non_executable_is_resolved() {
    let a = serde_json::to_value(assess(&units(Some("non_executable")), &judgment(0.98))).unwrap();
    assert_eq!(a["resolved_non_executable"], json!(["no-push"]));
    for kind in [None, Some("executable")] {
        let a = serde_json::to_value(assess(&units(kind), &judgment(0.98))).unwrap();
        assert_eq!(a["resolved_non_executable"], json!([]));
    }
    let a = serde_json::to_value(assess(&units(Some("non_executable")), &judgment(0.7))).unwrap();
    assert_eq!(a["resolved_non_executable"], json!([]));
}

#[test]
fn classification_evidence_is_kept_whole_in_the_existing_semantic_decision() {
    let requirement = json!({"id":"history","authority_items":["goal"],"checks":[],
        "citations":[{"quote":"User prohibition."},{"quote":"Whole observed history supporting classification."}],
        "judgment":{"kind":"semantic","decision":"satisfied","reasoning":"Complied","evidence_citations":[0]},
        "obligation":{"kind":"non_executable","reasoning":"Historical observation","evidence_citations":[1]}});
    let input = json!({"intent":{"goal":"Do not push."},"event":{"head":"sha"},"decision":{"requirements":{"history":requirement}}});
    let state =
        kah_observer::decision_units::context::decision(&input, "requirement.history").unwrap();
    assert_eq!(
        state["source_manifest"]["selected_citations"],
        json!([0, 1])
    );
    assert_eq!(
        state["decision"]["requirements"]["history"]["citations"]["1"]["quote"],
        "Whole observed history supporting classification."
    );
}
