use kah_core::{current_rules::*, intent_history::*};
use kah_observer::rule_source::{CurrentSet, Source as RuleSource};
use serde_json::json;
fn history() -> History {
    History::new(GoalRevision::new(
        1,
        "Preserve inputs",
        vec![SourceRef::new(
            SourceProvider::Codex,
            "worker",
            "u1",
            SourceRole::User,
            "sha256:test",
        )],
    ))
    .unwrap()
}
fn input(h: &History, text: &str) -> Input {
    Input::from_history(
        h,
        vec![Source::standing(
            "AGENTS.md",
            text,
            json!({"scope":"project"}),
        )],
        "now",
    )
    .unwrap()
}
fn candidate(input: &Input, previous: Option<&ValidatedSet>) -> RuleSet {
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
            disposition: if s.id.starts_with("goal:") {
                Disposition::Context
            } else {
                Disposition::Contributes
            },
            reason: "exact current source".into(),
            rules: if s.id.starts_with("goal:") {
                vec![]
            } else {
                vec![s.id.clone()]
            },
            witnesses: vec![],
        })
        .collect::<Vec<_>>();
    RuleSet {
        schema_version: 1,
        input_digest: input.digest().unwrap(),
        previous_digest: previous.map(|s| s.digest().unwrap()),
        rules: clauses
            .iter()
            .filter(|c| !c.rules.is_empty())
            .map(|c| Rule {
                id: c.id.clone(),
                predicate: c.words.quote.clone(),
                scope: "changed code in this project".into(),
                diff_checkable: true,
                clauses: vec![c.id.clone()],
                permissions: vec![],
            })
            .collect(),
        clauses,
    }
}
#[test]
fn current_source_changes_require_rebuild_before_the_next_commit_can_use_rules() {
    let h = history();
    let old = input(&h, "Never truncate inputs.");
    let first = candidate(&old, None).validate(&old, None).unwrap();
    let source = CurrentSet::new(&first, &old, "same-observer")
        .unwrap()
        .load(&[])
        .unwrap();
    assert_eq!(source.rules.len(), 1);
    let fresh = input(&h, "Never truncate inputs. Preserve all fields.");
    assert!(CurrentSet::new(&first, &fresh, "same-observer").is_err());
    let second = candidate(&fresh, Some(&first))
        .validate(&fresh, Some(&first))
        .unwrap();
    let source = CurrentSet::new(&second, &fresh, "same-observer")
        .unwrap()
        .load(&[])
        .unwrap();
    assert_eq!(source.source_digest, second.digest().unwrap());
    assert_eq!(
        source.rules[0].predicate,
        "Never truncate inputs. Preserve all fields."
    );
    assert_eq!(source.rules[0].scope, "changed code in this project");
    assert_eq!(source.observer_session, "same-observer");
}
#[test]
fn rule_permissions_stay_exact_and_local_with_multiple_source_links() {
    let i = input(
        &history(),
        "Never truncate inputs unless the user requests a preview.",
    );
    let mut c = candidate(&i, None);
    let quote = "unless the user requests a preview.";
    let s = i
        .sources
        .iter()
        .find(|s| s.id.starts_with("standing:"))
        .unwrap();
    let start = s.text.find(quote).unwrap();
    c.rules[0].permissions.push(Words {
        source: s.id.clone(),
        start,
        end: start + quote.len(),
        quote: quote.into(),
    });
    let v = c.validate(&i, None).unwrap();
    let r = CurrentSet::new(&v, &i, "same-observer")
        .unwrap()
        .load(&[])
        .unwrap();
    assert_eq!(r.rules[0].permissions.len(), 1);
    assert_eq!(r.rules[0].permissions[0].quote, quote);
    assert_eq!(r.source_words[&r.rules[0].sources[0].source_id], s.text);
    assert!(CurrentSet::new(&v, &i, "").is_err());
}
