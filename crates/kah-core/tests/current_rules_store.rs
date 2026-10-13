use kah_core::{
    current_rules::*,
    intent_history::*,
    projection::ActiveProjection,
    run_store::{RuleSync, RunStore},
};
use serde_json::json;
use std::{fs, path::PathBuf};
struct Fixture(PathBuf);
impl Drop for Fixture {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}
fn fixture() -> Fixture {
    Fixture(std::env::temp_dir().join(format!(
            "kah-rules-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        )))
}
fn standing(text: &str) -> Vec<Source> {
    vec![Source::standing(
        "AGENTS.md",
        text,
        json!({"scope":"project"}),
    )]
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
            disposition: Disposition::Contributes,
            reason: "Current obligation".into(),
            rules: vec![s.id.clone()],
            witnesses: vec![],
        })
        .collect::<Vec<_>>();
    RuleSet {
        schema_version: 1,
        input_digest: input.digest().unwrap(),
        previous_digest: previous.map(|v| v.digest().unwrap()),
        rules: clauses
            .iter()
            .map(|c| Rule {
                id: c.id.clone(),
                predicate: c.words.quote.clone(),
                scope: "project".into(),
                diff_checkable: true,
                kind: Some(kah_core::current_rules::RuleKind::Prohibition),
                clauses: vec![c.id.clone()],
                permissions: vec![],
            })
            .collect(),
        clauses,
    }
}
#[test]
fn changed_ledger_or_standing_input_requires_rebuild_and_rejects_stale_adoption() {
    let (_fixture, store, mut history, source) = open_fixture();
    let RuleSync::Rebuild(first) = store
        .sync_current_rules(standing("Never push"), "1")
        .unwrap()
    else {
        panic!("must rebuild");
    };
    assert!(first.direct_observer_review);
    let set = candidate(&first.input, None);
    let accepted = store
        .accept_current_rules(&set, standing("Never push"), "2")
        .unwrap();
    assert!(matches!(
        store
            .sync_current_rules(standing("Never push"), "3")
            .unwrap(),
        RuleSync::Ready(_)
    ));
    let RuleSync::Rebuild(changed) = store
        .sync_current_rules(standing("Never install"), "4")
        .unwrap()
    else {
        panic!("standing change must rebuild");
    };
    assert_eq!(changed.previous_digest, Some(accepted.digest().unwrap()));
    let stale = candidate(&changed.input, Some(&accepted));
    append_intent(&store, &mut history, source);
    assert!(
        store
            .accept_current_rules(&stale, standing("Never install"), "5")
            .is_err()
    );
    let RuleSync::Rebuild(fresh) = store
        .sync_current_rules(standing("Never install"), "6")
        .unwrap()
    else {
        panic!("ledger change must rebuild");
    };
    let next = candidate(&fresh.input, Some(&accepted));
    let next = store
        .accept_current_rules(&next, standing("Never install"), "7")
        .unwrap();
    assert_eq!(next.diff(Some(&accepted)).added, ["intent:later"]);
    assert_eq!(store.recover().unwrap().history, history);
    assert_eq!(
        store
            .accepted_current_rules()
            .unwrap()
            .unwrap()
            .digest()
            .unwrap(),
        next.digest().unwrap()
    );
    let path = store.path().join("current-rules").join(format!(
        "{}.json",
        next.digest().unwrap().strip_prefix("sha256:").unwrap()
    ));
    fs::write(path, "{}").unwrap();
    assert!(store.accepted_current_rules().is_err());
}

fn open_fixture() -> (Fixture, RunStore, History, SourceRef) {
    let fixture = fixture();
    let project = fixture.0.join("project");
    fs::create_dir_all(&project).unwrap();
    let source = SourceRef::new(
        SourceProvider::Bundle,
        "session",
        "goal",
        SourceRole::User,
        "sha256:goal",
    );
    let history = History::new(GoalRevision::new(1, "Build kah", vec![source.clone()])).unwrap();
    let projection =
        ActiveProjection::from_history(&history, kah_core::projection::ProjectionConfig::default())
            .unwrap();
    let store = RunStore::create(
        fixture.0.join("state"),
        &project,
        "run",
        &history,
        &projection,
    )
    .unwrap();
    (fixture, store, history, source)
}

fn append_intent(store: &RunStore, history: &mut History, source: SourceRef) {
    history
        .append(Event::RequirementAdded {
            item: IntentItem::new(
                "later",
                IntentKind::Constraint,
                "Retain evidence",
                vec![source],
            ),
            approval: None,
        })
        .unwrap();
    store
        .append_pending(history.records().last().unwrap().clone())
        .unwrap();
    store
        .commit_projection(
            &ActiveProjection::from_history(
                history,
                kah_core::projection::ProjectionConfig::default(),
            )
            .unwrap(),
        )
        .unwrap();
}
