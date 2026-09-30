use kah_core::{intent_history::*, projection::*};
use kah_observer::compaction::{self, CompactorConfig};
fn projection() -> ActiveProjection {
    let source = SourceRef::new(
        SourceProvider::Codex,
        "session",
        "u1",
        SourceRole::User,
        "sha256:abc",
    );
    let mut history = History::new(GoalRevision::new(
        1,
        "Exact original goal",
        vec![source.clone()],
    ))
    .unwrap();
    history
        .append(Event::RequirementAdded {
            item: IntentItem::new(
                "a",
                IntentKind::Constraint,
                " Exact raw steering\r\n ",
                vec![source],
            ),
            approval: None,
        })
        .unwrap();
    ActiveProjection::from_history(&history, ProjectionConfig::default()).unwrap()
}
#[test]
fn no_model_call_under_size_budget_and_goal_stays_raw() {
    let p = projection();
    let error = compaction::compact(
        std::path::Path::new("/does/not/exist"),
        &p,
        CompactorConfig::default(),
    )
    .unwrap_err();
    assert!(error.starts_with("OBSERVER_COMPACTION_UNNEEDED"));
}
#[test]
fn compaction_links_originals_and_cannot_change_ledger_or_survive_new_input() {
    let mut p = projection();
    let original = p.frontier.clone();
    let c = CompactedIntent {
        source_digest: compaction::digest(&p),
        summary: "Generated summary".into(),
        originals: vec!["a".into()],
        model: "fake".into(),
    };
    compaction::attach(&mut p, Some(&c));
    assert!(p.rendered_prompt().contains("Generated summary"));
    assert!(p.rendered_prompt().contains("Exact original goal"));
    assert_eq!(p.frontier, original);
    p.frontier[0].review_required = true;
    assert!(p.needs_review());
    p.frontier[0].text.push('!');
    compaction::attach(&mut p, Some(&c));
    assert!(p.compaction.is_none());
}

#[test]
#[ignore = "real size-triggered Luna synthesis, separate from Jev journey baseline"]
fn live_size_triggered_compaction_retains_verbatim_originals() {
    let mut p = projection();
    p.frontier[0].text = "Preserve marker BETA and do not add dependencies. ".repeat(900);
    let original = p.frontier.clone();
    let (summary, metadata) = compaction::compact(
        &std::env::current_dir().unwrap(),
        &p,
        CompactorConfig::default(),
    )
    .unwrap();
    compaction::attach(&mut p, Some(&summary));
    assert!(p.rendered_bytes() <= p.overflow.budget);
    assert_eq!(p.frontier, original);
    assert_eq!(p.goal.text, "Exact original goal");
    assert_eq!(summary.originals, vec!["a"]);
    assert!(summary.summary.contains("BETA"));
    assert_eq!(metadata.calls, 1);
    let path = std::path::PathBuf::from(
        std::env::var_os("KAH_GATE_EVIDENCE").expect("gate evidence directory"),
    );
    std::fs::write(path.join("compaction.json"), serde_json::to_string_pretty(&serde_json::json!({"passed":true,"rawBytes":original[0].text.len(),"renderedBytes":p.rendered_bytes(),"originalsUnchanged":true,"summary":summary,"metadata":metadata})).unwrap()).unwrap();
}
