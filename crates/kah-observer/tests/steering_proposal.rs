use kah_observer::{
    judge::{Answer, Judgment, Metrics},
    steering_proposal::{self, Proposal},
};
use serde_json::json;
use std::collections::BTreeMap;

fn proposal() -> Proposal {
    serde_json::from_value(json!({"source_record":"user-original","directs":true,"kind":"scope_reduction","supersedes":"older","reasoning":"The user explicitly removed this deliverable.","citations":[],"newer_conflicts":[]})).unwrap()
}
fn judgment(choice: &str) -> Judgment {
    Judgment {
        backend: "jev".into(),
        model: "jev-latest".into(),
        answers: BTreeMap::from([(
            "steering.proposal".into(),
            Answer::Choice {
                choice: choice.into(),
                confidence: 0.95,
                probabilities: BTreeMap::from([(choice.into(), 0.95)]),
            },
        )]),
        metrics: Metrics::default(),
        fallback_reason: None,
    }
}
#[test]
fn a_thinker_operation_needs_typed_jev_validation_without_invented_probabilities() {
    let candidate = proposal();
    assert!(steering_proposal::validated(
        &candidate,
        &judgment("supported")
    ));
    assert!(!steering_proposal::validated(
        &candidate,
        &judgment("unclear")
    ));
    assert!(!steering_proposal::validated(
        &candidate,
        &judgment("contradicted")
    ));
    let mut encoded = serde_json::to_value(candidate).unwrap();
    encoded["probability"] = json!(1.0);
    assert!(serde_json::from_value::<Proposal>(encoded).is_err());
}
#[test]
fn each_later_conflict_requires_its_own_validated_semantic_decision() {
    let mut value = serde_json::to_value(proposal()).unwrap();
    value["newer_conflicts"] = json!([{"id":"later","conflicts":true,"reasoning":"Both directions cannot apply together.","citations":[]}]);
    let candidate: Proposal = serde_json::from_value(value).unwrap();
    assert!(!steering_proposal::validated(
        &candidate,
        &judgment("supported")
    ));
    assert!(steering_proposal::questions(&candidate).contains_key("steering.conflict.later"));
}

#[test]
fn source_chronology_and_complete_conflict_coverage_are_host_checked() {
    use kah_observer::review_evidence::Document;
    let document = Document::new(
        "ledger",
        "ledger",
        "kah:ledger",
        "The user explicitly removed this deliverable.",
    );
    let mut candidate = proposal();
    candidate.citations = vec![
        serde_json::from_value(
            json!({"source":"ledger","digest":document.digest,"quote":document.text}),
        )
        .unwrap(),
    ];
    let documents = vec![document];
    assert!(
        candidate
            .verify(&documents, "user-original", &["older".into()], &[])
            .is_ok()
    );
    assert!(
        candidate
            .verify(&documents, "forged-source", &["older".into()], &[])
            .is_err()
    );
    assert!(
        candidate
            .verify(&documents, "user-original", &[], &[])
            .is_err()
    );
    assert!(
        candidate
            .verify(
                &documents,
                "user-original",
                &["older".into()],
                &["later".into()]
            )
            .is_err()
    );
}

#[test]
fn validated_operation_copies_verbatim_words_without_synthetic_scores() {
    use kah_core::intent_history::{
        Event, GoalRevision, History, IntentItem, IntentKind, SourceProvider, SourceRef, SourceRole,
    };
    let make_source = |record: &str| {
        SourceRef::new(
            SourceProvider::Codex,
            "session",
            record,
            SourceRole::User,
            format!("sha256:{}", "a".repeat(64)),
        )
    };
    let mut history = History::new(GoalRevision::new(
        1,
        "Original goal",
        vec![make_source("goal")],
    ))
    .unwrap();
    history
        .append(Event::RequirementAdded {
            item: IntentItem::new(
                "older",
                IntentKind::Constraint,
                "Publish the artifact.",
                vec![make_source("older-source")],
            ),
            approval: None,
        })
        .unwrap();
    let source = make_source("user-original");
    let text = "For this task, do not publish the artifact.";
    let original = history.clone();
    assert!(
        kah_observer::verbatim::apply_validated(
            &mut history,
            text,
            source.clone(),
            &proposal(),
            &judgment("unclear")
        )
        .is_err()
    );
    assert_eq!(history.records().len(), original.records().len());
    kah_observer::verbatim::apply_validated(
        &mut history,
        text,
        source.clone(),
        &proposal(),
        &judgment("supported"),
    )
    .unwrap();
    assert!(!history.intent("older").unwrap().is_active());
    let added = history
        .intent(kah_observer::verbatim::item_id(&source))
        .unwrap();
    assert_eq!(added.text, text);
    assert_eq!(added.introduced_by, vec![source]);
    assert!(!added.review_required);
}
