use kah_observer::{
    decision_units::{self, ReviewUnits},
    judge::{Answer, Judgment, Metrics},
    review_evidence::{Document, NativeRecord},
};
use serde_json::{Value, json};
use std::collections::BTreeMap;

fn units(facts: Vec<Value>) -> (ReviewUnits, Document) {
    let mut doc = Document::new(
        "native",
        "worker_transcript",
        "native:command",
        "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\nPARSER_PASS\nExit code: 0",
    );
    doc.native_records = vec![NativeRecord {
        start_byte: 0,
        end_byte: doc.text.len(),
        context: json!({"record_type":"commandExecution"}),
    }];
    let requirements = facts.into_iter().enumerate().map(|(i,fact)| json!({
        "id":format!("receipt-{i}"),"name":format!("Receipt property {i}"),"dimension":"integration","authority_items":["goal"],
        "citations":[{"source":doc.id,"digest":doc.digest,"quote":doc.text}],"components":[],
        "checks":[{"name":"parser","command_citation":0,"result_citation":0,"sha_citation":0,"sha":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"}],
        "judgment":{"kind":"atomic_fact","fact":fact,"evidence_citations":[0]}
    })).collect::<Vec<_>>();
    (
        serde_json::from_value(json!({"requirements":requirements})).unwrap(),
        doc,
    )
}

#[test]
fn each_receipt_property_has_a_literal_question_and_all_are_required() {
    let (units, doc) = units(vec![
        json!({"property":"sha","check":0,"expected":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"}),
        json!({"property":"exit_status","check":0,"expected":0}),
        json!({"property":"output_line","check":0,"expected":"PARSER_PASS"}),
    ]);
    units.verify(&[doc], &["goal".into()]).unwrap();
    let questions = decision_units::questions(&units);
    assert_eq!(questions.len(), 3);
    let answers = (0..3)
        .map(|i| {
            (
                format!("requirement.receipt-{i}"),
                Answer::Choice {
                    choice: "yes".into(),
                    confidence: 0.95,
                    probabilities: BTreeMap::from([("yes".into(), 0.95)]),
                },
            )
        })
        .collect();
    let mut judgment = Judgment {
        backend: "jev".into(),
        model: "jev-latest".into(),
        answers,
        metrics: Metrics::default(),
        fallback_reason: None,
    };
    assert!(decision_units::assess(&units, &judgment).passed);
    judgment.answers.remove("requirement.receipt-1");
    assert_eq!(
        decision_units::assess(&units, &judgment).needs_thinker,
        ["receipt-1"]
    );
}

#[test]
fn multiple_properties_and_multiline_output_cannot_hide_in_one_fact() {
    let compound = json!({"property":"sha","check":0,"expected":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","exit_status":0});
    assert!(serde_json::from_value::<decision_units::Literal>(compound).is_err());
    let (units, doc) = units(vec![
        json!({"property":"output_line","check":0,"expected":"PARSER_PASS\nRENDERER_PASS"}),
    ]);
    assert!(
        units
            .verify(&[doc], &["goal".into()])
            .unwrap_err()
            .starts_with("REVIEW_LITERAL_LINE")
    );
}

#[test]
fn a_missing_receipt_is_rejected_before_question_rendering() {
    let (units, doc) = units(vec![
        json!({"property":"exit_status","check":1,"expected":0}),
    ]);
    assert!(
        units
            .verify(&[doc], &["goal".into()])
            .unwrap_err()
            .starts_with("REVIEW_LITERAL_RECEIPT")
    );
}
