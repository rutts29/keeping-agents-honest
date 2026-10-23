//! A conflict resolution's chosen words are quotable rule authority; the
//! replaced wording stays in the history but cannot support a current rule.
use kah_core::{current_rules::*, intent_history::*};

fn user(record: &str) -> SourceRef {
    SourceRef::new(
        SourceProvider::Bundle,
        "session",
        record,
        SourceRole::User,
        format!("sha256:{record}"),
    )
}

fn resolved_input() -> Input {
    let mut history = History::new(GoalRevision::new(1, "Build kah", vec![user("goal")])).unwrap();
    history
        .append(Event::RequirementAdded {
            item: IntentItem::new(
                "i-1",
                IntentKind::Constraint,
                "keep alpha",
                vec![user("intent")],
            ),
            approval: None,
        })
        .unwrap();
    history
        .append(Event::ConflictRaised {
            conflict: Conflict::new(
                "c-1",
                vec![IntentId::new("i-1")],
                vec![
                    ConflictAlternative::new("alpha", "keep alpha", vec![user("alt-1")]),
                    ConflictAlternative::new("beta", "use beta", vec![user("alt-2")]),
                ],
                vec![user("conflict")],
            ),
            approval: None,
        })
        .unwrap();
    history
        .append(Event::ConflictResolved {
            conflict_id: "c-1".into(),
            resolution: ConflictResolution::new("beta", vec![user("resolution")], None),
        })
        .unwrap();
    Input::from_history(&history, vec![], "2026-10-23T00:00:00Z").unwrap()
}

fn words(source: &Source) -> Words {
    Words {
        source: source.id.clone(),
        start: 0,
        end: source.text.len(),
        quote: source.text.clone(),
    }
}

/// Every source contributes its own rule, except `superseded`, which cites
/// `witness` as what replaced it.
fn rule_set(input: &Input, superseded: Option<(&str, &Source)>) -> RuleSet {
    let clauses = input
        .sources
        .iter()
        .map(|s| {
            let replaced = superseded.filter(|(id, _)| *id == s.id);
            Clause {
                id: s.id.clone(),
                words: words(s),
                disposition: if replaced.is_some() {
                    Disposition::Superseded
                } else {
                    Disposition::Contributes
                },
                reason: "Current obligation".into(),
                rules: if replaced.is_some() {
                    vec![]
                } else {
                    vec![s.id.clone()]
                },
                witnesses: replaced.map(|(_, w)| vec![words(w)]).unwrap_or_default(),
            }
        })
        .collect::<Vec<_>>();
    let rules = clauses
        .iter()
        .filter(|c| !c.rules.is_empty())
        .map(|c| Rule {
            id: c.id.clone(),
            predicate: c.words.quote.clone(),
            scope: "project".into(),
            diff_checkable: true,
            kind: Some(RuleKind::Prohibition),
            clauses: vec![c.id.clone()],
            permissions: vec![],
        })
        .collect();
    RuleSet {
        schema_version: 1,
        input_digest: input.digest().unwrap(),
        previous_digest: None,
        rules,
        clauses,
    }
}

#[test]
fn a_resolution_is_quotable_and_the_replaced_words_cannot_support_a_rule() {
    // The rule input labelled the original "keep alpha" active and had no
    // source for the chosen "use beta".
    let input = resolved_input();
    assert!(
        input
            .history
            .iter()
            .any(|r| matches!(r.event, Event::ConflictResolved { .. }))
    );
    let resolution = input
        .sources
        .iter()
        .find(|s| s.text == "use beta")
        .expect("the chosen wording is a source");
    assert_eq!(resolution.origin["current_lifecycle"], "active");
    let original = input.sources.iter().find(|s| s.id == "intent:i-1").unwrap();
    assert_eq!(original.text, "keep alpha", "history words are kept");
    assert_ne!(original.origin["current_lifecycle"], "active");

    let error = rule_set(&input, None).validate(&input, None).unwrap_err();
    assert!(error.contains("LIFECYCLE"), "{error}");
    rule_set(&input, Some(("intent:i-1", resolution)))
        .validate(&input, None)
        .unwrap();
}
