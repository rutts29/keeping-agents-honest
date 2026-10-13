//! Exact eight retained sources; metadata-only native adoption, no providers.
use kah_cli::observer_review::option1::current_set;
use kah_core::{
    current_rules::*,
    intent_history::*,
    projection::{ActiveProjection, ProjectionConfig},
    run_store::{RuleSync, RunStore},
};
use kah_observer::{intake::Input as Intake, session};
use serde_json::json;
use std::fs;

fn fixture() -> (std::path::PathBuf, session::Binding, Vec<Intake>) {
    std::env::var_os("XDG_STATE_HOME").expect("run through demos/check-tests.sh");
    let root = std::env::temp_dir().join(format!("intake-eight-{}", kah_cli::relay_store::nonce()));
    fs::create_dir(&root).unwrap();
    let records: Vec<Intake> =
        serde_json::from_str(include_str!("fixtures/current-intake-eight.json")).unwrap();
    let binding = session::Binding {
        session: format!("intake-eight-{}", kah_cli::relay_store::nonce()),
        project: root.clone(),
        codex_home: root.join("codex"),
        native: "codex".into(),
        model: "gpt-6.1-sol".into(),
        effort: "low".into(),
        timeout_secs: 90,
        judge: Default::default(),
        jev_model: "jev-1.13.0".into(),
    };
    let h = History::new(GoalRevision::new(
        1,
        "Keep all original source evidence",
        vec![records[0].source.clone()],
    ))
    .unwrap();
    let base = RunStore::default_state_root().unwrap();
    drop(
        RunStore::create(
            &base,
            &root,
            "run",
            &h,
            &ActiveProjection::from_history(&h, ProjectionConfig::default()).unwrap(),
        )
        .unwrap(),
    );
    let mut state = session::State::new(binding.clone());
    state.run_id = Some("run".into());
    let mut primary = session::Store::open_wait(&base, &binding.session).unwrap();
    primary.save(&state).unwrap();
    for input in &records {
        primary
            .append(&json!({"kind":"user_source_v1","classifier_version":1,"input":input}))
            .unwrap();
    }
    drop(primary);
    drop(kah_cli::observer_review::state::Store::open(&binding.session).unwrap());
    (root, binding, records)
}

fn candidate(r: &kah_core::run_store::RuleRebuild) -> RuleSet {
    let clauses = r
        .input
        .sources
        .iter()
        .map(|s| {
            let rejected = s.origin["non_authority_sender"] == true;
            Clause {
                id: s.id.clone(),
                words: Words {
                    source: s.id.clone(),
                    start: 0,
                    end: s.text.len(),
                    quote: s.text.clone(),
                },
                disposition: if rejected {
                    Disposition::RejectedJunk
                } else if s.id.starts_with("goal:") {
                    Disposition::Context
                } else {
                    Disposition::Contributes
                },
                reason: if rejected {
                    "Unreceipted builder coordination, not user authority"
                } else {
                    "Complete original user source accounted in its scope"
                }
                .into(),
                rules: if rejected || s.id.starts_with("goal:") {
                    vec![]
                } else {
                    vec![s.id.clone()]
                },
                witnesses: vec![],
            }
        })
        .collect::<Vec<_>>();
    let rules = clauses
        .iter()
        .filter(|c| !c.rules.is_empty())
        .map(|c| Rule {
            id: c.id.clone(),
            predicate: c.words.quote.clone(),
            scope: "original source scope".into(),
            diff_checkable: true,
            kind: Some(kah_core::current_rules::RuleKind::Prohibition),
            clauses: vec![c.id.clone()],
            permissions: vec![],
        })
        .collect();
    RuleSet {
        schema_version: 1,
        input_digest: r.input.digest().unwrap(),
        previous_digest: r.previous_digest.clone(),
        rules,
        clauses,
    }
}

#[test]
fn adopting_the_eight_records_settles_intake_without_user_authority_or_history_rewrite() {
    let (root, binding, original) = fixture();
    assert!(
        current_set::resolve(&binding.session, "observer", "HEAD")
            .unwrap()
            .is_none()
    );
    let Some(RuleSync::Rebuild(rebuild)) = kah_observer::current_rules::stage(&binding).unwrap()
    else {
        panic!("missing rebuild")
    };
    let set = candidate(&rebuild);
    let result = current_set::accept(&binding.session, "observer", &set).unwrap();
    assert_eq!(result["status"], "validated");
    let primary =
        session::Store::open_wait(&RunStore::default_state_root().unwrap(), &binding.session)
            .unwrap();
    let sources = primary.sources().unwrap();
    assert_eq!(sources.iter().filter(|s| s.promoted).count(), 6);
    assert_eq!(sources.iter().filter(|s| s.dismissed).count(), 2);
    for (old, new) in original.iter().zip(&sources) {
        assert_eq!(old.source, new.source);
        assert_eq!(old.text, new.text);
    }
    drop(primary);
    assert!(
        current_set::resolve(&binding.session, "observer", "HEAD")
            .unwrap()
            .is_some()
    );
    assert_eq!(
        current_set::accept(&binding.session, "observer", &set).unwrap()["intake_resolution"],
        json!({"promoted":0,"dismissed":0,"authority":"observer evidence; history and stored classification unchanged"})
    );
    let store = RunStore::open_default(&binding.project, "run").unwrap();
    assert_eq!(
        store.recover().unwrap().history.records(),
        rebuild.input.history
    );
    drop(store);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn a_record_absent_from_the_validated_input_stays_pending() {
    let (root, binding, original) = fixture();
    let Some(RuleSync::Rebuild(rebuild)) = kah_observer::current_rules::stage(&binding).unwrap()
    else {
        panic!("missing rebuild")
    };
    let set = candidate(&rebuild).validate(&rebuild.input, None).unwrap();
    let mut late = original[0].clone();
    late.local_record = Some("late-unaccounted-source".into());
    let base = RunStore::default_state_root().unwrap();
    let mut primary = session::Store::open_wait(&base, &binding.session).unwrap();
    primary.add_source(&late).unwrap();
    drop(primary);
    kah_observer::current_rules::intake::resolve(&binding, &set, &rebuild.input).unwrap();
    let primary = session::Store::open_wait(&base, &binding.session).unwrap();
    let records = primary.sources().unwrap();
    let pending = records.iter().find(|r| r.key() == late.key()).unwrap();
    assert!(!pending.promoted && !pending.dismissed && pending.rule_set_resolution.is_none());
    drop(primary);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn legacy_delegate_records_cannot_support_rules_even_if_the_model_calls_them_user_intent() {
    let (root, binding, _) = fixture();
    let Some(RuleSync::Rebuild(rebuild)) = kah_observer::current_rules::stage(&binding).unwrap()
    else {
        panic!("missing rebuild")
    };
    let mut set = candidate(&rebuild);
    let clause = set
        .clauses
        .iter_mut()
        .find(|c| c.disposition == Disposition::RejectedJunk)
        .unwrap();
    clause.disposition = Disposition::Contributes;
    clause.rules = vec![clause.id.clone()];
    set.rules.push(Rule {
        id: clause.id.clone(),
        predicate: clause.words.quote.clone(),
        scope: "claimed user scope".into(),
        diff_checkable: true,
        kind: Some(kah_core::current_rules::RuleKind::Prohibition),
        clauses: vec![clause.id.clone()],
        permissions: vec![],
    });
    assert!(
        set.validate(&rebuild.input, None)
            .unwrap_err()
            .starts_with("RULE_SET_AUTHORITY")
    );
    fs::remove_dir_all(root).unwrap();
}
