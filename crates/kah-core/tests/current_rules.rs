use kah_core::{current_rules::*, intent_history::*};
use serde_json::json;

fn input() -> Input {
    let source = SourceRef::new(
        SourceProvider::Bundle,
        "session",
        "user",
        SourceRole::User,
        "sha256:user",
    );
    let mut history =
        History::new(GoalRevision::new(1, "Build kah", vec![source.clone()])).unwrap();
    history
        .append(Event::RequirementAdded {
            item: IntentItem::new("junk", IntentKind::Constraint, "-builder", vec![source]),
            approval: None,
        })
        .unwrap();
    Input::from_history(
        &history,
        vec![Source::standing(
            "AGENTS.md",
            "Never push. Except with explicit approval.",
            json!({"scope":"project"}),
        )],
        "2026-10-11T00:00:00Z",
    )
    .unwrap()
}
fn candidate(input: &Input) -> RuleSet {
    let clauses = input
        .sources
        .iter()
        .map(|s| Clause {
            id: s.id.clone(),
            words: Words {
                source: s.id.clone(),
                start: 0,
                end: s.text.len(),
                quote: s.text.clone(),
            },
            disposition: if s.id == "intent:junk" {
                Disposition::RejectedJunk
            } else {
                Disposition::Contributes
            },
            reason: if s.id == "intent:junk" {
                "No directive in this capture"
            } else {
                "Current obligation"
            }
            .into(),
            rules: if s.id == "intent:junk" {
                vec![]
            } else {
                vec![s.id.clone()]
            },
            witnesses: vec![],
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
            clauses: vec![c.id.clone()],
            permissions: if c.id == "standing:AGENTS.md" {
                vec![Words {
                    source: c.words.source.clone(),
                    start: 12,
                    end: c.words.end,
                    quote: "Except with explicit approval.".into(),
                }]
            } else {
                vec![]
            },
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
fn full_history_and_standing_words_are_accounted_and_permissions_are_rule_local() {
    let input = input();
    let set = candidate(&input);
    let validated = set.validate(&input, None).unwrap();
    let questions = validated.questions("abc123").unwrap();
    assert_eq!(questions.len(), 2);
    assert!(questions[0].permissions.is_empty());
    assert_eq!(questions[1].permissions, ["Except with explicit approval."]);
    assert_eq!(validated.diff(None).added.len(), 2);
    assert_eq!(validated.diff(Some(&validated)).added.len(), 0);
    assert_eq!(
        input
            .sources
            .iter()
            .find(|s| s.id == "intent:junk")
            .unwrap()
            .text,
        "-builder"
    );
}
#[test]
fn malformed_or_stale_projection_never_becomes_a_question_set() {
    let input = input();
    let original = candidate(&input);
    let mut mutations = Vec::new();
    let mut s = original.clone();
    s.clauses.pop();
    mutations.push(s);
    let mut s = original.clone();
    s.clauses[0].words.quote.push('x');
    mutations.push(s);
    let mut s = original.clone();
    s.rules[0].clauses.clear();
    mutations.push(s);
    let mut s = original.clone();
    s.clauses[1].disposition = Disposition::Superseded;
    mutations.push(s);
    let mut s = original.clone();
    s.rules[0].permissions = s.rules[1].permissions.clone();
    mutations.push(s);
    let mut s = original.clone();
    s.input_digest = "stale".into();
    mutations.push(s);
    for set in mutations {
        assert!(set.validate(&input, None).is_err());
    }
    let previous = original.validate(&input, None).unwrap();
    let mut next = original.clone();
    next.previous_digest = Some(previous.digest().unwrap());
    next.rules[0].id = "unnecessary-renaming".into();
    assert!(next.validate(&input, Some(&previous)).is_err());
}
