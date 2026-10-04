use kah_cli::observer_review::{
    state::{Purpose, Request},
    steering,
};
use kah_core::{
    intent_history::{SourceProvider, SourceRef, SourceRole},
    run_store::RunStore,
};
use kah_observer::{
    intake::{Decision, Input},
    judge::{Answer, Judgment, Metrics},
    review_evidence::Document,
    session::{Binding, State, Store},
    steering_proposal::Proposal,
};
use serde_json::json;
use std::{collections::BTreeMap, time::Duration};
fn judgment() -> Judgment {
    Judgment {
        backend: "jev".into(),
        model: "jev-latest".into(),
        answers: BTreeMap::from([(
            "steering.proposal".into(),
            Answer::Choice {
                choice: "supported".into(),
                confidence: 0.95,
                probabilities: BTreeMap::from([("supported".into(), 0.95)]),
            },
        )]),
        metrics: Metrics::default(),
        fallback_reason: None,
    }
}
fn input(record: &str) -> Input {
    Input {
        source: SourceRef::new(
            SourceProvider::Codex,
            "session",
            record,
            SourceRole::User,
            format!("sha256:{}", "a".repeat(64)),
        ),
        local_record: None,
        text: format!("Direction {record}."),
        decision: Some(Decision::Verification {
            judgment: Box::new(judgment()),
        }),
        dismissed: false,
        promoted: false,
        prior_intent: Some(
            json!({"goal":"Older goal","goal_item_id":"older","items":[{"id":"older","text":"Original requirement."}]}),
        ),
        newer_conflicts: vec![],
    }
}
fn setup(label: &str) -> (std::path::PathBuf, String, Input) {
    let base = RunStore::default_state_root().unwrap();
    let worker = format!("steering-proposal-unit-{label}-{}", std::process::id());
    let mut store = Store::open(&base, &worker, Duration::ZERO).unwrap();
    let home = std::env::temp_dir().join(&worker);
    store
        .save(&State::new(Binding {
            session: worker.clone(),
            project: home.clone(),
            codex_home: home.join("codex"),
            native: home.join("native"),
            model: "gpt-6.1-sol".into(),
            effort: "low".into(),
            timeout_secs: 45,
            jev_model: "jev-latest".into(),
            judge: kah_observer::judge::routing::Backend::Jev,
        }))
        .unwrap();
    let source = input("original");
    store.add_source(&source).unwrap();
    (base, worker, source)
}
fn request(worker: &str, source: &Input) -> Request {
    Request {
        id: "review-steering".into(),
        worker: worker.into(),
        purpose: Purpose::Steering,
        turn: "t".into(),
        head: None,
        intent: json!({}),
        trigger: json!({"uncertain_user_input":source,"steering_context":steering::context(worker,source).unwrap()}),
        documents: vec![Document::new("ledger", "ledger", "kah:ledger", "null")],
        created_ms: 0,
    }
}
fn proposal(source: &Input) -> Proposal {
    serde_json::from_value(json!({"source_record":source.source.record,"directs":true,"kind":"new_constraint","supersedes":null,"reasoning":"The original message adds direction.","citations":[],"newer_conflicts":[]})).unwrap()
}
#[test]
fn validated_proposal_is_persisted_without_legacy_score_translation() {
    let (base, worker, source) = setup("persist");
    let request = request(&worker, &source);
    assert_eq!(
        steering::apply_proposal(&request, &proposal(&source), &judgment()).unwrap(),
        "verified_for_next_native_event"
    );
    let store = Store::open(&base, &worker, Duration::ZERO).unwrap();
    let sources = store.sources().unwrap();
    assert!(matches!(
        sources[0].decision,
        Some(Decision::Validated { .. })
    ));
    let directory = Store::directory(&base, &worker);
    drop(store);
    std::fs::remove_dir_all(directory).unwrap();
}
#[test]
fn later_direction_during_review_requires_a_recheck_and_is_never_an_earlier_prior() {
    let (base, worker, source) = setup("chronology");
    let request = request(&worker, &source);
    let mut store = Store::open(&base, &worker, Duration::ZERO).unwrap();
    let later = input("later");
    store.add_source(&later).unwrap();
    drop(store);
    let context = steering::context(&worker, &source).unwrap();
    assert_eq!(context["prior_intent"]["items"][0]["id"], "older");
    assert_eq!(context["newer"][0]["id"], later.id());
    assert_eq!(
        steering::apply_proposal(&request, &proposal(&source), &judgment()).unwrap(),
        "intent_changed_recheck_next_turn"
    );
    let store = Store::open(&base, &worker, Duration::ZERO).unwrap();
    assert!(matches!(
        store.sources().unwrap()[0].decision,
        Some(Decision::Verification { .. })
    ));
    let directory = Store::directory(&base, &worker);
    drop(store);
    std::fs::remove_dir_all(directory).unwrap();
}
