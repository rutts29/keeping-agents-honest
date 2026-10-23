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

fn requirement(history: &mut History, id: &str, text: &str) {
    history
        .append(Event::RequirementAdded {
            item: IntentItem::new(id, IntentKind::Constraint, text, vec![user(id)]),
            approval: None,
        })
        .unwrap();
}

fn conflict(history: &mut History, id: &str, intents: &[&str], texts: [&str; 2]) {
    history
        .append(Event::ConflictRaised {
            conflict: Conflict::new(
                id,
                intents.iter().map(|i| IntentId::new(*i)).collect(),
                vec![
                    ConflictAlternative::new("first", texts[0], vec![user("alt-1")]),
                    ConflictAlternative::new("second", texts[1], vec![user("alt-2")]),
                ],
                vec![user(id)],
            ),
            approval: None,
        })
        .unwrap();
}

fn resolve(history: &mut History, id: &str) {
    history
        .append(Event::ConflictResolved {
            conflict_id: id.into(),
            resolution: ConflictResolution::new("second", vec![user("resolution")], None),
        })
        .unwrap();
}

/// The source carrying `text`, for `intent` when it is a resolution's words.
fn source<'a>(input: &'a Input, text: &str, intent: Option<&str>) -> &'a Source {
    input
        .sources
        .iter()
        .find(|s| s.text == text && intent.is_none_or(|i| s.origin["intent_id"] == i))
        .unwrap_or_else(|| panic!("no source for {text}"))
}

/// Each listed source gets its disposition and one witness; every other
/// source contributes its own rule.
fn account(input: &Input, plan: &[(&Source, Disposition, &Source)]) -> RuleSet {
    let mut set = rule_set(input, None);
    for (source, disposition, witness) in plan {
        let clause = set.clauses.iter_mut().find(|c| c.id == source.id).unwrap();
        clause.disposition = disposition.clone();
        clause.rules.clear();
        clause.witnesses = vec![words(witness)];
        set.rules.retain(|r| r.id != source.id);
    }
    set
}

#[test]
fn a_later_conflict_is_accounted_on_the_items_current_resolved_words() {
    // After "keep alpha" was resolved to "use beta", a new conflict on the
    // same item could only be accounted on the replaced "keep alpha" words.
    let mut history = History::new(GoalRevision::new(1, "Build kah", vec![user("goal")])).unwrap();
    requirement(&mut history, "i-1", "keep alpha");
    requirement(&mut history, "i-2", "use gamma");
    conflict(&mut history, "c-1", &["i-1"], ["keep alpha", "use beta"]);
    resolve(&mut history, "c-1");
    conflict(&mut history, "c-2", &["i-1"], ["use beta", "use gamma"]);
    let input = Input::from_history(&history, vec![], "2026-10-23T00:00:00Z").unwrap();
    let (alpha, beta) = (
        source(&input, "keep alpha", None),
        source(&input, "use beta", None),
    );
    let gamma = source(&input, "use gamma", None);
    assert_eq!(beta.origin["current_lifecycle"], "conflicted");

    let accounted = account(
        &input,
        &[
            (alpha, Disposition::Superseded, beta),
            (beta, Disposition::UnresolvedConflict, gamma),
        ],
    );
    let validated = accounted.validate(&input, None).unwrap();
    assert!(validated.has_conflicts());
    assert!(validated.questions("abc123").is_err());
    // The replaced words cannot stand in for the item's current words.
    let stale = account(
        &input,
        &[
            (alpha, Disposition::UnresolvedConflict, gamma),
            (beta, Disposition::Context, gamma),
        ],
    );
    let error = stale.validate(&input, None).unwrap_err();
    assert!(error.contains("CONFLICT"), "{error}");
}

#[test]
fn each_item_of_a_shared_resolution_keeps_its_own_current_words() {
    // One resolution rewords two items; a later conflict affects only one.
    let mut history = History::new(GoalRevision::new(1, "Build kah", vec![user("goal")])).unwrap();
    requirement(&mut history, "i-1", "keep alpha");
    requirement(&mut history, "i-3", "keep alpha too");
    requirement(&mut history, "i-2", "use gamma");
    conflict(
        &mut history,
        "c-1",
        &["i-1", "i-3"],
        ["keep alpha", "use beta"],
    );
    resolve(&mut history, "c-1");
    conflict(&mut history, "c-2", &["i-3"], ["use beta", "use gamma"]);
    let input = Input::from_history(&history, vec![], "2026-10-23T00:00:00Z").unwrap();
    let alpha = source(&input, "keep alpha", None);
    let alpha_too = source(&input, "keep alpha too", None);
    let gamma = source(&input, "use gamma", None);
    let (beta_1, beta_3) = (
        source(&input, "use beta", Some("i-1")),
        source(&input, "use beta", Some("i-3")),
    );
    assert_ne!(beta_1.id, beta_3.id);
    assert_eq!(beta_1.origin["current_lifecycle"], "active");
    assert_eq!(beta_3.origin["current_lifecycle"], "conflicted");
    let replaced = [
        (alpha, Disposition::Superseded, beta_1),
        (alpha_too, Disposition::Superseded, beta_3),
    ];
    let mut plan = replaced.to_vec();
    plan.push((beta_3, Disposition::UnresolvedConflict, gamma));
    account(&input, &plan).validate(&input, None).unwrap();
    // The other item's words do not account for this conflict.
    let mut plan = replaced.to_vec();
    plan.push((beta_1, Disposition::UnresolvedConflict, gamma));
    plan.push((beta_3, Disposition::Context, gamma));
    let error = account(&input, &plan).validate(&input, None).unwrap_err();
    assert!(error.contains("CONFLICT"), "{error}");
}
