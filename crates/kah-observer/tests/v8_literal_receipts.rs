use kah_observer::{
    decision_units::{self, ReviewUnits},
    judge::{Answer, Judgment, Metrics},
    review_evidence::{Document, NativeRecord},
};
use serde_json::{Value, json};
use std::collections::BTreeMap;

fn report(fact: Value, quote: Option<&str>) -> (ReviewUnits, Document) {
    let text = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\nPARSER_PASS\nExit code: 0";
    let mut doc = Document::new("native", "worker_transcript", "native:command", text);
    doc.native_records = vec![NativeRecord {
        start_byte: 0,
        end_byte: text.len(),
        context: json!({"record_type":"commandExecution","exit_code":0,"status":"completed","output_ranges":[[0,text.len()]]}),
    }];
    let units=serde_json::from_value(json!({"version":8,"requirements":[{"id":"receipt","name":"One native receipt property","dimension":"integration","authority_items":["goal"],"blocker":null,"citations":[{"source":doc.id,"digest":doc.digest,"quote":quote.unwrap_or(text)}],"components":[],"checks":[{"name":"parser","command_citation":0,"result_citation":0,"sha_citation":0,"sha":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"}],"applicability":{"due":true,"unconditional":true,"phase_completion":true,"reasoning":"Unconditional exact fact.","evidence_citations":[0]},"judgment":{"kind":"atomic_fact","fact":fact,"evidence_citations":[0]}}]})).unwrap();
    (units, doc)
}

#[test]
fn contradictory_literal_claims_are_rejected_before_judging() {
    for fact in [
        json!({"property":"sha","check":0,"expected":"bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"}),
        json!({"property":"exit_status","check":0,"expected":1}),
        json!({"property":"output_line","check":0,"expected":"INTEGRATION_PASS"}),
    ] {
        let (units, doc) = report(fact, None);
        assert!(
            units
                .verify(&[doc], &["goal".into()])
                .unwrap_err()
                .starts_with("REVIEW_LITERAL_CONTRADICTS")
        );
    }
}

#[test]
fn exact_matching_receipt_cannot_be_turned_into_a_violation_by_jev() {
    for fact in [
        json!({"property":"sha","check":0,"expected":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"}),
        json!({"property":"exit_status","check":0,"expected":0}),
        json!({"property":"output_line","check":0,"expected":"PARSER_PASS"}),
    ] {
        let (units, doc) = report(fact, None);
        units.verify(&[doc], &["goal".into()]).unwrap();
        assert!(decision_units::questions(&units).is_empty());
        let judgment = Judgment {
            backend: "jev".into(),
            model: "jev-latest".into(),
            answers: BTreeMap::from([(
                "requirement.receipt".into(),
                Answer::Choice {
                    choice: "no".into(),
                    confidence: 1.0,
                    probabilities: BTreeMap::from([("no".into(), 1.0)]),
                },
            )]),
            metrics: Metrics::default(),
            fallback_reason: None,
        };
        assert!(decision_units::assess(&units, &judgment).passed);
    }
}
