use kah_core::intent_history::{
    GoalRevision, History, IntentLifecycle, SourceProvider, SourceRef, SourceRole,
};
use kah_observer::{
    chronology,
    intake::{Decision, Input},
    judge::{Answer, Judgment, Metrics},
    relayed_user::Message,
    verbatim,
};
use serde_json::Value;
use std::collections::BTreeMap;
fn input(source: SourceRef, text: &str, decision: Decision) -> Input {
    Input {
        source,
        text: text.into(),
        local_record: None,
        decision: Some(decision),
        dismissed: false,
        promoted: false,
        prior_intent: None,
        newer_conflicts: vec![],
    }
}
fn judgment() -> Judgment {
    Judgment {
        backend: "jev".into(),
        model: "jev-1.13.0".into(),
        answers: BTreeMap::from([
            ("is_steering".into(), Answer::Noul { noul: 0.99 }),
            (
                "steer_kind".into(),
                Answer::Choice {
                    choice: "new_constraint".into(),
                    probabilities: BTreeMap::from([("new_constraint".into(), 1.0)]),
                    confidence: 1.0,
                },
            ),
        ]),
        metrics: Metrics::default(),
        fallback_reason: None,
    }
}
fn actual() -> Vec<Input> {
    let rows: Vec<Value> =
        serde_json::from_str(include_str!("fixtures/reversal-sources-20261003.json")).unwrap();
    rows.into_iter()
        .map(|row| {
            input(
                serde_json::from_value(row["source"].clone()).unwrap(),
                row["text"].as_str().unwrap(),
                Decision::Verification {
                    judgment: Box::new(judgment()),
                },
            )
        })
        .collect()
}
fn goal() -> SourceRef {
    SourceRef::new(
        SourceProvider::Codex,
        "fixture",
        "goal",
        SourceRole::User,
        "digest",
    )
}
#[test]
fn real_deferred_serialization_cannot_become_active_after_its_linked_reversal() {
    let mut sources = actual();
    sources[1].source.supersedes_receipt = Some("r0000000000000001-1".into());
    let mut history = History::new(GoalRevision::new(1, "Complete Phase5O", vec![goal()])).unwrap();
    chronology::apply_links(&mut history, &sources).unwrap();
    assert_eq!(
        history.intent(sources[0].id()).unwrap().text,
        sources[0].text
    );
    assert_eq!(
        history.intent(sources[0].id()).unwrap().lifecycle,
        IntentLifecycle::Withdrawn
    );
    assert!(history.intent(sources[1].id()).unwrap().is_active());
    verbatim::apply(
        &mut history,
        &sources[0].text,
        sources[0].source.clone(),
        &judgment(),
    )
    .unwrap();
    assert!(!history.intent(sources[0].id()).unwrap().is_active());
    assert_eq!(
        history.intent(sources[0].id()).unwrap().changed_by,
        [sources[1].source.clone()]
    );
}
#[test]
fn deferred_typing_never_sees_newer_items_and_unlinked_conflicts_are_explicit() {
    let mut sources = actual();
    let mut history = History::new(GoalRevision::new(1, "Complete Phase5O", vec![goal()])).unwrap();
    for source in &sources {
        kah_observer::intake::force(&mut history, &source.text, source.source.clone()).unwrap();
    }
    let prior = chronology::before(Some(&history), &sources, sources[0].key()).unwrap();
    assert!(prior["items"].as_array().unwrap().is_empty());
    let newer_id = sources[1].id();
    sources[0].newer_conflicts.push(newer_id);
    chronology::apply_conflicts(&mut history, &sources).unwrap();
    assert_eq!(history.open_conflicts().len(), 1);
    assert_eq!(
        history.intent(sources[0].id()).unwrap().lifecycle,
        IntentLifecycle::Conflicted
    );
}
#[test]
fn explicit_link_roundtrips_in_the_header_and_source_provenance() {
    let mut message = Message::new("kah-reviewer", "new", " User exact words\n").unwrap();
    message.supersedes = Some("old".into());
    let parsed = Message::parse(&message.wire()).unwrap().unwrap();
    assert_eq!(parsed.text, message.text);
    assert_eq!(parsed.supersedes, Some("old".into()));
    assert_eq!(
        parsed
            .quote("worker", SourceProvider::Codex)
            .unwrap()
            .source
            .supersedes_receipt,
        Some("old".into())
    );
    assert!(message.wire().contains(" receipt=new supersedes=old\n"));
    assert!(
        Message::parse("[kah-user] relayer=kah-reviewer receipt=new supersedes=bad receipt\ntext")
            .is_err()
    );
}

#[test]
fn historical_context_does_not_inherit_a_future_conflict_resolution() {
    use kah_core::intent_history::{Conflict, ConflictAlternative, ConflictResolution, Event};
    let make = |id: &str, text: &str| {
        input(
            SourceRef::new(SourceProvider::Codex, "fixture", id, SourceRole::User, id),
            text,
            Decision::Force,
        )
    };
    let sources = vec![
        make("older-a", "Keep observers on."),
        make("older-b", "Keep Codex serialized."),
        make("pending", "Review the earlier directive."),
        make("newer", "Choose observers on."),
    ];
    let mut history = History::new(GoalRevision::new(
        1,
        "Complete5O",
        vec![sources[0].source.clone()],
    ))
    .unwrap();
    for source in &sources[..2] {
        kah_observer::intake::force(&mut history, &source.text, source.source.clone()).unwrap();
    }
    let alternatives = sources[..2]
        .iter()
        .enumerate()
        .map(|(n, s)| ConflictAlternative::new(n.to_string(), &s.text, vec![s.source.clone()]))
        .collect();
    let conflict = Conflict::new(
        "older-conflict",
        sources[..2].iter().map(|s| s.id().into()).collect(),
        alternatives,
        sources[..2].iter().map(|s| s.source.clone()).collect(),
    );
    history
        .append(Event::ConflictRaised {
            conflict,
            approval: None,
        })
        .unwrap();
    history
        .append(Event::ConflictResolved {
            conflict_id: "older-conflict".into(),
            resolution: ConflictResolution::new("0", vec![sources[3].source.clone()], None),
        })
        .unwrap();
    let prior = chronology::before(Some(&history), &sources, sources[2].key()).unwrap();
    assert!(
        prior["items"].as_array().unwrap().is_empty(),
        "the conflict was still open at the input, despite today's resolved state"
    );
    assert_eq!(prior["goal"], "Complete5O");
}
