use kah_cli::dispatch::filterable;
use kah_core::intent_history::{IntentItem, IntentKind, SourceProvider, SourceRef, SourceRole};
use kah_observer::{
    intake::{Decision, Input},
    judge::{Answer, Judgment},
};
use std::collections::BTreeMap;

fn input(category: &str) -> Input {
    Input {
        prior_intent: None,
        newer_conflicts: vec![],
        source: SourceRef::new(
            SourceProvider::Codex,
            "parent",
            "user",
            SourceRole::User,
            "digest",
        ),
        local_record: None,
        text: "Original user instruction".into(),
        decision: Some(Decision::Jev {
            judgment: Box::new(Judgment {
                backend: "jev".into(),
                model: "fixture".into(),
                answers: BTreeMap::from([(
                    "steer_kind".into(),
                    Answer::Choice {
                        choice: category.into(),
                        probabilities: BTreeMap::from([(category.into(), 1.0)]),
                        confidence: 1.0,
                    },
                )]),
                metrics: Default::default(),
                fallback_reason: None,
            }),
        }),
        dismissed: false,
        promoted: false,
        rule_set_resolution: None,
    }
}
#[test]
fn relevance_cannot_omit_constraints_prohibitions_corrections_or_reductions() {
    for kind in [
        IntentKind::Constraint,
        IntentKind::Invariant,
        IntentKind::Validation,
        IntentKind::StopCondition,
    ] {
        let source = input("new_constraint");
        let item = IntentItem::new(
            "item",
            kind,
            "Never delete protected files",
            vec![source.source.clone()],
        );
        assert!(!filterable(&item, &source));
    }
    for category in ["correction", "scope_reduction"] {
        let source = input(category);
        for kind in [
            IntentKind::Constraint,
            IntentKind::Scope,
            IntentKind::Outcome,
        ] {
            let item = IntentItem::new(
                "item",
                kind,
                "Original instruction",
                vec![source.source.clone()],
            );
            assert!(!filterable(&item, &source));
        }
    }
}
#[test]
fn only_clear_goal_or_scope_expansion_is_filterable() {
    let mut source = input("scope_expansion");
    let mut item = IntentItem::new(
        "item",
        IntentKind::Scope,
        "Also build a second deliverable",
        vec![source.source.clone()],
    );
    assert!(filterable(&item, &source));
    item.kind_uncertain = true;
    assert!(!filterable(&item, &source));
    item.kind_uncertain = false;
    source.decision = Some(Decision::Inherited {
        item: Box::new(item.clone()),
    });
    assert!(!filterable(&item, &source));
    item.kind = IntentKind::Outcome;
    assert!(filterable(&item, &source));
    item.review_required = true;
    assert!(!filterable(&item, &source));
}
