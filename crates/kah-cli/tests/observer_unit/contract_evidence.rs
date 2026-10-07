use super::super::followup::{
    Binding, BuilderOutcome, BuilderReceipt, EvidencePointer, Stage, VerificationRound,
};
use super::*;
use kah_observer::{
    review_evidence::{Document, Report},
    verification_contract::{Item, Method},
};
#[test]
fn evidence_and_asks_are_specific_to_the_item_and_keep_whole_checked_quotes() {
    let doc = Document::new(
        "normalizer",
        "committed_code",
        "git:HEAD:normalize.cjs",
        "module.exports = xs => xs.map(n => Math.round(n * 100) / 100);\n",
    );
    let report:Report=serde_json::from_value(json!({"request_id":"review-proof","findings":[],"decision_units":{"version":10,"requirements":[{"id":"normalize","name":"Normalization","dimension":"integration","authority_items":["goal"],"citations":[{"source":doc.id,"digest":doc.digest,"quote":doc.text}],"components":[],"checks":[],"judgment":{"kind":"semantic","decision":"satisfied","reasoning":"Export matches the exact request.","evidence_citations":[0]}}]}})).unwrap();
    let request = Request {
        id: "review-proof".into(),
        worker: "worker".into(),
        purpose: Purpose::Checkpoint,
        turn: "turn".into(),
        head: Some("a".repeat(40)),
        intent: json!({"revision":1}),
        trigger: json!({}),
        documents: vec![doc.clone()],
        created_ms: 1,
    };
    let item = Item {
        requirement_id: "normalize".into(),
        claim: "Rounding export".into(),
        evidence_citations: vec![0],
        coverage_reasoning: "The source names this precise export.".into(),
        method: Method::NeedsCheck {
            why: "Need an executable assertion.".into(),
        },
    };
    let round = round(&request);
    let packet = selected(&request, &report, &item).unwrap();
    assert_eq!(packet[0]["quote"], doc.text);
    let tried = attempts(
        &request,
        &report,
        &item,
        &round,
        &json!({"needs_thinker":["normalize"]}),
    )
    .unwrap();
    assert!(tried[0].what.contains("git:HEAD:normalize.cjs"));
    assert!(tried[1].what.contains("private:native-reply"));
    let unasked = attempts(
        &request,
        &report,
        &item,
        &round,
        &json!({"needs_thinker":[]}),
    )
    .unwrap();
    assert_eq!(unasked.len(), 1);
    let mut changed = request;
    changed.documents[0].text = "changed source".into();
    assert!(selected(&changed, &report, &item).is_err());
}

fn round(request: &Request) -> VerificationRound {
    let pointer = EvidencePointer {
        receipt: "builder".into(),
        record: "private:native-reply".into(),
        digest: "a".repeat(64),
    };
    VerificationRound {
        id: "followup-proof".into(),
        binding: Binding::from_request(request).unwrap(),
        stage: Stage::Checks,
        claimed: true,
        evidence: Some(vec![pointer.clone()]),
        builder: Some(BuilderReceipt {
            request: pointer.clone(),
            response: BuilderOutcome::Evidence { response: pointer },
        }),
        checks: None,
    }
}
