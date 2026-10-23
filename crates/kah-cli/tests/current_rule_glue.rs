use kah_cli::observer_review::{
    option1::{current_set, input, protocol, requests},
    queue,
    state::{Purpose, Request},
};
use kah_core::{
    current_rules::*,
    intent_history::*,
    projection::{ActiveProjection, ProjectionConfig},
    run_store::{RuleSync, RunStore},
};
use kah_observer::session;
use serde_json::{Value, json};
use std::{fs, path::PathBuf};
struct Fixture {
    root: PathBuf,
    binding: session::Binding,
    history: History,
}
impl Drop for Fixture {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.root).unwrap();
    }
}
fn fixture() -> Fixture {
    let root = std::env::temp_dir().join(format!("current-glue-{}", kah_cli::relay_store::nonce()));
    let project = root.join("project");
    fs::create_dir_all(&project).unwrap();
    fs::write(project.join("AGENTS.md"), "Never truncate inputs.").unwrap();
    let h = History::new(GoalRevision::new(1, "Preserve inputs", vec![source()])).unwrap();
    // Run through demos/check-tests.sh to isolate the default state root.
    let base = RunStore::default_state_root().unwrap();
    let worker = format!("current-glue-{}", kah_cli::relay_store::nonce());
    let binding = session::Binding {
        session: worker,
        project: project.clone(),
        codex_home: root.join("codex"),
        native: "codex".into(),
        model: "gpt-6.1-sol".into(),
        effort: "low".into(),
        timeout_secs: 90,
        judge: Default::default(),
        jev_model: "jev-1.13.0".into(),
    };
    drop(
        RunStore::create(
            &base,
            &project,
            "run",
            &h,
            &ActiveProjection::from_history(&h, ProjectionConfig::default()).unwrap(),
        )
        .unwrap(),
    );
    let mut state = session::State::new(binding.clone());
    state.run_id = Some("run".into());
    session::Store::open_wait(&base, &binding.session)
        .unwrap()
        .save(&state)
        .unwrap();
    drop(kah_cli::observer_review::state::Store::open(&binding.session).unwrap());
    Fixture {
        root,
        binding,
        history: h,
    }
}
fn source() -> SourceRef {
    SourceRef::new(
        SourceProvider::Codex,
        "worker",
        "u1",
        SourceRole::User,
        "sha256:test",
    )
}
fn rebuild(f: &Fixture) -> Box<kah_core::run_store::RuleRebuild> {
    let Some(RuleSync::Rebuild(r)) = kah_observer::current_rules::stage(&f.binding).unwrap() else {
        panic!("expected rebuild")
    };
    r
}
fn candidate(r: &kah_core::run_store::RuleRebuild) -> RuleSet {
    let clauses = r
        .input
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
            reason: "current source".into(),
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
        input_digest: r.input.digest().unwrap(),
        previous_digest: r.previous_digest.clone(),
        rules: clauses
            .iter()
            .filter(|c| !c.rules.is_empty())
            .map(|c| Rule {
                id: c.id.clone(),
                predicate: c.words.quote.clone(),
                scope: "this project".into(),
                diff_checkable: true,
                kind: Some(kah_core::current_rules::RuleKind::Prohibition),
                clauses: vec![c.id.clone()],
                permissions: vec![],
            })
            .collect(),
        clauses,
    }
}
fn adopt(f: &Fixture, r: &kah_core::run_store::RuleRebuild) -> Value {
    let c = candidate(r);
    let req = Request {
        id: "rebuild".into(),
        worker: f.binding.session.clone(),
        purpose: Purpose::Steering,
        turn: "t".into(),
        head: None,
        intent: Value::Null,
        trigger: json!({"option1":{"stage":"current_rules","rebuild":r}}),
        documents: vec![],
        created_ms: 0,
    };
    protocol::validate(
        &req,
        &serde_json::to_vec(
            &json!({"request":req.id,"result":{"stage":"current_rules","candidate":c}}),
        )
        .unwrap(),
    )
    .unwrap();
    current_set::accept(&f.binding.session, "same-observer", &c).unwrap()
}
#[test]
fn intent_change_rebuild_is_deduplicated_validated_and_used_by_next_commit() {
    let mut f = fixture();
    let worker = f.binding.session.clone();
    let first = rebuild(&f);
    assert!(
        current_set::resolve(&worker, "same-observer", &"a".repeat(40))
            .unwrap()
            .is_none()
    );
    current_set::enqueue(&f.binding, "t").unwrap();
    current_set::enqueue(&f.binding, "t").unwrap();
    assert_eq!(queue::entries(&worker).unwrap().len(), 1);
    let (claimed, _) = queue::claim_batch(&worker)
        .unwrap()
        .expect("rebuild must be admissible without uncertain-message payload");
    assert_eq!(claimed.trigger["kind"], "current_rules_rebuild");
    assert_eq!(adopt(&f, &first)["status"], "validated");
    assert_eq!(
        adopt(&f, &first)["status"],
        "validated",
        "replayed ACK is idempotent after adoption"
    );
    assert!(
        current_set::resolve(&worker, "another-observer", &"a".repeat(40))
            .unwrap()
            .is_none()
    );
    f.history
        .append(Event::RequirementAdded {
            item: IntentItem::new(
                "new",
                IntentKind::Constraint,
                "Never strip fields",
                vec![source()],
            ),
            approval: None,
        })
        .unwrap();
    let store = RunStore::open_default(&f.binding.project, "run").unwrap();
    store
        .append_pending(f.history.records().last().unwrap().clone())
        .unwrap();
    store
        .commit_projection(
            &ActiveProjection::from_history(&f.history, ProjectionConfig::default()).unwrap(),
        )
        .unwrap();
    drop(store);
    assert!(
        current_set::resolve(&worker, "same-observer", &"b".repeat(40))
            .unwrap()
            .is_none()
    );
    assert_eq!(
        current_set::accept(&worker, "same-observer", &candidate(&first)).unwrap()["status"],
        "stale_candidate_retained"
    );
    let next = rebuild(&f);
    assert_eq!(adopt(&f, &next)["status"], "validated");
    next_commit(&f, &worker);
}
fn next_commit(f: &Fixture, worker: &str) {
    let source = current_set::resolve(worker, "same-observer", &"b".repeat(40))
        .unwrap()
        .unwrap();
    let ctx = json!({"head":"b".repeat(40),"baseline_tree":"a".repeat(40),"touched_files":[{"path":"lib.js","patch":"+preserve(value)","deleted":false,"content":{"text":"preserve(value)\n"}}]});
    let input = input::build(&f.history, &source, &ctx, false).unwrap();
    let (body, _) = requests::build(&input, None).unwrap();
    assert_eq!(body.len(), 1);
    assert_eq!(input.rules.len(), 2);
    assert!(body[0].body.contains("Never strip fields"));
    assert!(body[0].body.contains("this project"));
}
#[test]
fn intake_gap_cannot_use_an_otherwise_current_set_and_direct_keeps_whole_sources() {
    let f = fixture();
    let r = rebuild(&f);
    adopt(&f, &r);
    // Run through demos/check-tests.sh to isolate the default state root.
    let base = RunStore::default_state_root().unwrap();
    let mut primary = session::Store::open_wait(&base, &f.binding.session).unwrap();
    let mut s = primary.load().unwrap().unwrap();
    s.gap = Some(session::Gap {
        code: "SOURCE_GAP".into(),
        detail: "unsettled user source".into(),
        at_ms: 0,
    });
    primary.save(&s).unwrap();
    drop(primary);
    assert!(
        current_set::resolve(&f.binding.session, "same-observer", &"a".repeat(40))
            .unwrap()
            .is_none()
    );
    let direct =
        kah_cli::observer_review::option1::direct::fallback(&r.input, "same-observer").unwrap();
    assert_eq!(direct.source_words.len(), r.input.sources.len());
    for s in r.input.sources {
        assert_eq!(direct.source_words[&s.id], s.text)
    }
    assert!(direct.rules.iter().all(|r| !r.diff_checkable));
    assert!(protocol::schema("current_rules")["result"]["candidate"]["clauses"].is_array());
}
#[test]
fn an_evidence_summary_or_withdrawn_item_cannot_answer_a_rule_conflict() {
    let mut s = Source::standing("source", "question-id choose complete input", json!({}));
    s.origin = json!({"kind":"history","event":{"type":"evidence_attached"},"sequence":20});
    assert!(!kah_cli::observer_review::option1::rule_questions::is_user_answer(&s));
    s.origin = json!({"kind":"history","event":{"type":"requirement_added"},"current_lifecycle":"withdrawn","sequence":21});
    assert!(!kah_cli::observer_review::option1::rule_questions::is_user_answer(&s));
    s.origin["current_lifecycle"] = "active".into();
    s.origin["event"]["item"] = json!({"introduced_by":[source()]});
    assert!(kah_cli::observer_review::option1::rule_questions::is_user_answer(&s));
    s.origin["event"]["item"]["introduced_by"][0]["role"] = "assistant".into();
    assert!(!kah_cli::observer_review::option1::rule_questions::is_user_answer(&s));
}
#[test]
fn startup_without_history_is_native_only_and_cannot_claim_an_empty_intent_pass() {
    let i = Input::from_history(
        &History::new(GoalRevision::new(1, "temporary test input", vec![source()])).unwrap(),
        vec![Source::standing("AGENTS.md", "Never truncate", json!({}))],
        "now",
    )
    .unwrap();
    let mut resolved = kah_cli::observer_review::option1::direct::fallback(&i, "native").unwrap();
    resolved.rules.retain(|r| {
        r.sources
            .iter()
            .all(|s| s.source_id.starts_with("standing:"))
    });
    let ctx = json!({"head":"a".repeat(40),"touched_files":[{"path":"lib.js","patch":"+preserve(value)","deleted":false,"content":{"text":"preserve(value)\n"}}]});
    let input = input::without_history(&resolved, &ctx).unwrap();
    assert_eq!(input.intent.goal, "");
    assert!(requests::build(&input, None).unwrap().0.is_empty());
    assert!(input.rules.contains_key("controller_intent_pending"));
}
#[test]
fn adopted_revision_two_then_new_constraint_queues_revision_three_without_a_stop() {
    use std::os::unix::fs::OpenOptionsExt;
    let mut f = fixture();
    append(&mut f, "first", "Never truncate text input.");
    let r2 = rebuild(&f);
    assert_eq!(r2.input.revision, 2);
    assert_eq!(adopt(&f, &r2)["intent_revision"], 2);
    let root = kah_cli::observer_review::state::Store::directory(&f.binding.session).unwrap();
    kah_cli::unattended_report::save(&root,"config.json",&json!({"enabled":true,"provider":"codex","worker":f.binding.session,"turn":"short-user-turn","pid":null,"process_stamp":null})).unwrap();
    // Hold the existing detached-kick lease: this $0 queue test cannot launch a
    // native process; it exercises the same sync boundary after ledger commit.
    let lock = fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .mode(0o600)
        .open(root.join("job-__review-kick.lock"))
        .unwrap();
    lock.lock().unwrap();
    append(&mut f, "second", "Never remove any object field.");
    kah_cli::observer_intake::after_sync(&f.binding, Default::default()).unwrap();
    let r3 = rebuild(&f);
    assert_eq!(r3.input.revision, 3);
    let queued = queue::entries(&f.binding.session).unwrap();
    assert_eq!(
        queued.len(),
        1,
        "staging revision3 must itself publish a rebuild wake"
    );
    assert_eq!(
        queued[0].1.trigger["input_digest"],
        r3.input.digest().unwrap()
    );
    kah_cli::observer_intake::after_sync(&f.binding, Default::default()).unwrap();
    assert_eq!(queue::entries(&f.binding.session).unwrap().len(), 1);
    drop(lock);
}
fn append(f: &mut Fixture, id: &str, text: &str) {
    f.history
        .append(Event::RequirementAdded {
            item: IntentItem::new(id, IntentKind::Constraint, text, vec![source()]),
            approval: None,
        })
        .unwrap();
    let s = RunStore::open_default(&f.binding.project, "run").unwrap();
    s.append_pending(f.history.records().last().unwrap().clone())
        .unwrap();
    s.commit_projection(
        &ActiveProjection::from_history(&f.history, ProjectionConfig::default()).unwrap(),
    )
    .unwrap();
}
#[test]
fn held_run_writer_is_pending_at_current_set_boundary_and_recovers_after_release() {
    let f = fixture();
    let r = rebuild(&f);
    adopt(&f, &r);
    let held = RunStore::open_default(&f.binding.project, "run").unwrap();
    assert!(
        current_set::resolve(&f.binding.session, "same-observer", &"a".repeat(40))
            .unwrap()
            .is_none()
    );
    drop(held);
    assert!(
        current_set::resolve(&f.binding.session, "same-observer", &"a".repeat(40))
            .unwrap()
            .is_some()
    );
}
