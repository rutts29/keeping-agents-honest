use kah_core::intent_history::{
    GoalRevision, History, IntentLifecycle, SourceProvider, SourceRef, SourceRole,
};
use kah_observer::{
    judge::{Answer, Judgment, Metrics},
    verbatim,
};
use std::collections::BTreeMap;

fn source(id: &str) -> SourceRef {
    SourceRef::new(
        SourceProvider::Codex,
        "session",
        id,
        SourceRole::User,
        format!("sha256:{id}"),
    )
}
fn judgment(kind: &str, target: &str, confidence: f64) -> Judgment {
    let choice = |value: &str| Answer::Choice {
        choice: value.into(),
        probabilities: BTreeMap::from([(value.into(), 0.99)]),
        confidence,
    };
    Judgment {
        backend: "jev".into(),
        model: "fake".into(),
        answers: BTreeMap::from([
            ("is_steering".into(), Answer::Noul { noul: 0.99 }),
            ("steer_kind".into(), choice(kind)),
            ("supersedes".into(), choice(target)),
        ]),
        metrics: Metrics::default(),
        fallback_reason: None,
    }
}
fn history() -> History {
    let mut h = History::new(GoalRevision::new(1, "Write ALPHA", vec![source("u1")])).unwrap();
    verbatim::apply(
        &mut h,
        "Write ALPHA",
        source("u1"),
        &judgment("new_constraint", "none", 0.99),
    )
    .unwrap();
    h
}
#[test]
fn exact_text_and_original_goal_survive_correction_and_replay() {
    let mut h = history();
    let old = h.intents().keys().next().unwrap().to_string();
    let text = "  Correction: βETA replaces ALPHA.\r\nKeep other constraints.  ";
    verbatim::apply(
        &mut h,
        text,
        source("u2"),
        &judgment("correction", &old, 0.99),
    )
    .unwrap();
    let h = History::from_jsonl(&h.to_jsonl().unwrap()).unwrap();
    assert_eq!(h.goal().text, "Write ALPHA");
    assert_eq!(
        h.intent(&old).unwrap().lifecycle,
        IntentLifecycle::Superseded
    );
    let current = h.intents().values().find(|i| i.is_active()).unwrap();
    assert_eq!(current.text, text);
    assert_eq!(current.introduced_by, vec![source("u2")]);
}
#[test]
fn uncertain_supersession_keeps_both_active_and_flagged() {
    let mut h = history();
    let old = h.intents().keys().next().unwrap().to_string();
    let mut j = judgment("correction", &old, 0.99);
    if let Answer::Choice { confidence, .. } = j.answers.get_mut("supersedes").unwrap() {
        *confidence = 0.4;
    }
    verbatim::apply(&mut h, "Maybe use BETA instead", source("u2"), &j).unwrap();
    assert_eq!(h.intents().values().filter(|i| i.is_active()).count(), 2);
    assert!(
        h.intents()
            .values()
            .any(|i| !i.uncertain_supersedes.is_empty())
    );
}
#[test]
fn withdrawal_is_source_linked_and_does_not_discard_exact_message() {
    let mut h = history();
    let old = h.intents().keys().next().unwrap().to_string();
    verbatim::apply(
        &mut h,
        "Do not write ALPHA anymore",
        source("u2"),
        &judgment("scope_reduction", &old, 0.99),
    )
    .unwrap();
    assert_eq!(
        h.intent(&old).unwrap().lifecycle,
        IntentLifecycle::Withdrawn
    );
    assert!(
        h.intents()
            .values()
            .any(|i| i.text == "Do not write ALPHA anymore")
    );
}
#[test]
fn question_and_non_user_sources_cannot_change_intent() {
    let mut h = history();
    let before = h.clone();
    let mut j = judgment("question_only", "none", 0.99);
    j.answers
        .insert("is_steering".into(), Answer::Noul { noul: 0.01 });
    verbatim::apply(&mut h, "Why?", source("u2"), &j).unwrap();
    assert_eq!(h, before);
    let mut s = source("u3");
    s.role = SourceRole::Assistant;
    assert!(verbatim::apply(&mut h, "BETA", s, &judgment("correction", "none", 0.99)).is_err());
    assert_eq!(h, before);
}
#[test]
fn source_records_preserve_crlf_and_whitespace() {
    let text = "  β\r\nnext\r  ";
    let r = kah_core::session_bundle::BundleRecord::new("u1", SourceRole::User, text).unwrap();
    assert_eq!(r.content(), text);
}

#[test]
fn reapplying_an_accepted_source_is_idempotent() {
    let mut h = history();
    let old = h.intents().keys().next().unwrap().to_string();
    let j = judgment("correction", &old, 0.99);
    verbatim::apply(&mut h, "Use BETA", source("u2"), &j).unwrap();
    let accepted = h.clone();
    verbatim::apply(&mut h, "Use BETA", source("u2"), &j).unwrap();
    assert_eq!(h, accepted);
}
#[test]
fn interrupted_withdrawal_finishes_without_repeating_the_operation() {
    let mut h = history();
    let old = h.intents().keys().next().unwrap().to_string();
    h.append(kah_core::intent_history::Event::RequirementWithdrawn {
        intent_id: old.clone().into(),
        source_refs: vec![source("u2")],
        approval: None,
    })
    .unwrap();
    verbatim::apply(
        &mut h,
        "Do not write ALPHA",
        source("u2"),
        &judgment("scope_reduction", &old, 0.99),
    )
    .unwrap();
    assert_eq!(h.intents().values().filter(|i| i.is_active()).count(), 1);
    assert!(h.intents().values().any(|i| i.text == "Do not write ALPHA"));
}

#[test]
fn uncertain_kind_preserves_clear_steering_without_replacing_anything() {
    let mut h = history();
    let old = h.intents().keys().next().unwrap().to_string();
    let mut j = judgment("correction", &old, 0.99);
    if let Answer::Choice { confidence, .. } = j.answers.get_mut("steer_kind").unwrap() {
        *confidence = 0.4;
    }
    verbatim::apply(
        &mut h,
        "Also BETA; preserve prior constraints",
        source("u2"),
        &j,
    )
    .unwrap();
    assert_eq!(h.intents().values().filter(|i| i.is_active()).count(), 2);
    assert!(
        h.intents()
            .values()
            .any(|i| !i.uncertain_supersedes.is_empty())
    );
}

#[test]
fn uncertain_presence_is_retained_for_review_without_supersession() {
    let mut h = history();
    let old = h.intents().keys().next().unwrap().to_string();
    let mut j = judgment("correction", &old, 0.99);
    j.answers
        .insert("is_steering".into(), Answer::Noul { noul: 0.63 });
    verbatim::apply(&mut h, "Remove the previously configured usage limit.", source("u2"), &j).unwrap();
    assert_eq!(h.intents().values().filter(|i| i.is_active()).count(), 2);
    assert!(h.intents().values().any(|i| i.review_required));
}
