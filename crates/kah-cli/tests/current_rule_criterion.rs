use kah_cli::observer_review::{
    option1::{prompt, protocol},
    state::{Purpose, Request},
};
use kah_core::{
    current_rules::Source,
    intent_history::{
        Event, GoalRevision, History, IntentItem, IntentKind, SourceProvider, SourceRef, SourceRole,
    },
    projection::{ActiveProjection, ProjectionConfig},
    run_store::{RuleRebuild, RuleSync, RunStore},
};
use serde_json::json;

/// A rule rebuild at revision 3: a goal and two later user requirements.
fn rebuild(root: &std::path::Path) -> RuleRebuild {
    let project = root.join("project");
    std::fs::create_dir_all(&project).unwrap();
    let user = |record: &str| {
        SourceRef::new(
            SourceProvider::Codex,
            "session",
            record,
            SourceRole::User,
            format!("sha256:{record}"),
        )
    };
    let goal = GoalRevision::new(1, "Build a CSV export command", vec![user("goal")]);
    let mut history = History::new(goal).unwrap();
    let projection = |h: &History| ActiveProjection::from_history(h, ProjectionConfig::default());
    let store = RunStore::create(
        root.join("state"),
        &project,
        "run",
        &history,
        &projection(&history).unwrap(),
    )
    .unwrap();
    for (id, text) in [
        (
            "columns",
            "Keep the columns in the order the user lists them.",
        ),
        (
            "no-network",
            "Never call the network from the export command.",
        ),
    ] {
        let item = IntentItem::new(id, IntentKind::Constraint, text, vec![user(id)]);
        history
            .append(Event::RequirementAdded {
                item,
                approval: None,
            })
            .unwrap();
        store
            .append_pending(history.records().last().unwrap().clone())
            .unwrap();
        store
            .commit_projection(&projection(&history).unwrap())
            .unwrap();
    }
    let standing = vec![Source::standing(
        "AGENTS.md",
        "Run the tests before every commit.",
        json!({"scope":"project"}),
    )];
    match store.sync_current_rules(standing, "1").unwrap() {
        RuleSync::Rebuild(rebuild) => *rebuild,
        RuleSync::Ready(_) => panic!("a new rule input must rebuild"),
    }
}

#[test]
fn rebuild_prompt_and_schema_include_the_same_checkability_criterion() {
    let root = std::env::temp_dir().join(format!("kah-criterion-{}", std::process::id()));
    let input = rebuild(&root);
    assert_eq!(input.input.revision, 3);
    let req = Request {
        id: "review-criterion".into(),
        worker: "fixture-worker".into(),
        purpose: Purpose::Steering,
        turn: "t".into(),
        head: None,
        intent: json!({}),
        trigger: json!({"option1":{"stage":"current_rules","input_digest":input.input.digest().unwrap(),"rebuild":input}}),
        documents: vec![],
        created_ms: 0,
    };
    let criterion = "a rule is diff-checkable when the code change alone can break it. Runtime measurements, investigations, sequencing and outside-design references are not diff-checkable";
    let p = prompt::compose(
        &req,
        std::path::Path::new("/inputs"),
        std::path::Path::new("/out/report.json"),
        "bin/kah observe report",
    )
    .unwrap();
    assert!(p.contains(criterion));
    assert_eq!(
        protocol::schema("current_rules")["checkability_criterion"],
        criterion
    );
    // This checks instruction wiring, not model judgment.
    std::fs::remove_dir_all(root).unwrap();
}
