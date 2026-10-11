use kah_core::{
    intent_history::{SourceProvider, SourceRole},
    observation::ObservationSnapshot,
    run_store::RunStore,
    session_bundle::{BundleRecord, NeutralSessionBundle},
};
use kah_observer::{
    engine,
    judge::routing::Backend,
    review,
    session::{Binding, Store},
};
use sha2::{Digest, Sha256};
use std::{
    fs,
    path::PathBuf,
    time::{SystemTime, UNIX_EPOCH},
};
struct Fixture {
    root: PathBuf,
    binding: Binding,
}
impl Fixture {
    fn new() -> Self {
        let id = format!(
            "kah-controls-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );
        let root = std::env::temp_dir().join(&id);
        fs::create_dir(&root).unwrap();
        let binding = Binding {
            session: id,
            project: root.clone(),
            codex_home: "/unused".into(),
            native: "/unused".into(),
            model: "gpt-6.1-sol".into(),
            effort: "low".into(),
            timeout_secs: 45,
            judge: Backend::Jev,
            jev_model: "jev-latest".into(),
        };
        Self { root, binding }
    }
    fn observe(&self, texts: &[&str]) -> engine::Outcome {
        let digest = format!(
            "sha256:{:x}",
            Sha256::digest(self.root.as_os_str().as_encoded_bytes())
        );
        let records = texts
            .iter()
            .enumerate()
            .map(|(i, t)| {
                BundleRecord::new(format!("kah-user:fixture:{i}"), SourceRole::User, *t).unwrap()
            })
            .collect();
        let users = NeutralSessionBundle::from_records(
            SourceProvider::Bundle,
            &self.binding.session,
            digest,
            records,
        )
        .unwrap();
        engine::synchronize_snapshot(
            &self.binding,
            "fixture",
            ObservationSnapshot {
                users: Some(users),
                actions: vec![],
                action_digest: format!("sha256:{:x}", Sha256::digest(b"[]")),
            },
        )
        .unwrap()
    }
    fn state(&self) -> kah_observer::session::State {
        Store::read(
            &RunStore::default_state_root().unwrap(),
            &self.binding.session,
        )
        .unwrap()
        .unwrap()
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let base = RunStore::default_state_root().unwrap();
        let repository = kah_core::run_store::repository_id(&self.root).unwrap();
        let _ = fs::remove_dir_all(base.join("repositories").join(repository));
        let _ = fs::remove_dir_all(Store::directory(&base, &self.binding.session));
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn quote(text: &str) -> kah_observer::inherited::Quote {
    use kah_core::intent_history::SourceRef;
    let record = BundleRecord::new("sender-user-1", SourceRole::User, text).unwrap();
    kah_observer::inherited::Quote {
        receipt: "relay-1".into(),
        source: SourceRef::new(
            SourceProvider::Claude,
            "sender",
            record.id(),
            SourceRole::User,
            record.content_digest(),
        ),
        text: text.into(),
        dispatch_item: None,
    }
}

#[test]
fn oversized_intent_stays_verbatim_without_a_headless_model() {
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "oversized_intent_probe", "--nocapture"])
        .env("KAH_OVERSIZED_INTENT_PROBE", "1")
        // The old path must fail before any native model can be launched.
        .env("KAH_CODEX_BIN", "/nonexistent/kah-forbidden-headless-model")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stdout)
    );
}

#[test]
fn oversized_intent_probe() {
    if std::env::var_os("KAH_OVERSIZED_INTENT_PROBE").is_none() {
        return;
    }
    let f = Fixture::new();
    let text = format!(
        "steer: {}",
        "Preserve the exact requirement and all its source references. ".repeat(700)
    );
    let outcome = f.observe(&[&text]);
    assert!(outcome.gap.is_none(), "{:?}", outcome.gap);
    let projection = outcome.projection.unwrap();
    assert!(!projection.is_overflowed());
    assert!(projection.compaction.is_none());
    assert!(projection.rendered_prompt().contains(&text[7..]));
    assert_eq!(f.state().usage.calls, 0);
    let mut legacy = f.state();
    legacy.compaction = Some(kah_core::projection::CompactedIntent {
        source_digest: kah_observer::compaction::digest(&projection),
        summary: "Obsolete shortened intent".into(),
        originals: vec![],
        model: "historical".into(),
    });
    let reread = engine::projection(&legacy).unwrap().unwrap();
    assert!(reread.compaction.is_none());
    assert!(reread.rendered_prompt().contains(&text[7..]));
}

#[test]
fn inherited_quotes_keep_origin_and_interleave_with_later_native_users() {
    let f = Fixture::new();
    f.observe(&["steer: Implement a greeting."]);
    let before = f.state().run_id;
    let quoted = quote("steer: Preserve BLUE exactly.");
    {
        let _store = Store::open(
            &RunStore::default_state_root().unwrap(),
            &f.binding.session,
            std::time::Duration::ZERO,
        )
        .unwrap();
        for _ in 0..2 {
            Store::accept_inherited(
                &RunStore::default_state_root().unwrap(),
                &f.binding.session,
                SourceProvider::Bundle,
                quoted.clone(),
            )
            .unwrap();
        }
    }
    let result = f.observe(&["steer: Implement a greeting."]);
    assert!(result.gap.is_none(), "{:?}", result.gap);
    assert!(
        result
            .projection
            .unwrap()
            .rendered_prompt()
            .contains("BLUE")
    );
    let result = f.observe(&["steer: Implement a greeting.", "steer: Add tests."]);
    assert!(result.gap.is_none(), "{:?}", result.gap);
    assert_eq!(before, f.state().run_id);
    let sources = {
        let store = Store::open(
            &RunStore::default_state_root().unwrap(),
            &f.binding.session,
            std::time::Duration::ZERO,
        )
        .unwrap();
        store.sources().unwrap()
    };
    assert_eq!(sources.len(), 3);
    assert_eq!(sources[1].source, quoted.source);
    assert_eq!(sources[1].text, quoted.text);
    review::apply(&f.binding.session, "dismiss", &sources[1].id()).unwrap();
    assert_eq!(before, f.state().run_id);
    assert!(
        !f.observe(&["steer: Implement a greeting.", "steer: Add tests."])
            .projection
            .unwrap()
            .rendered_prompt()
            .contains("BLUE")
    );
    assert!(
        f.observe(&["steer: Rewrite history."])
            .gap
            .unwrap()
            .contains("SOURCE_REWRITE")
    );
}

#[test]
fn a_verified_quote_can_seed_a_delegate_only_session_without_promoting_delegate_text() {
    let f = Fixture::new();
    let base = RunStore::default_state_root().unwrap();
    {
        Store::accept_inherited(
            &base,
            &f.binding.session,
            SourceProvider::Bundle,
            quote("steer: Retain exact  spacing."),
        )
        .unwrap();
    }
    let result = engine::synchronize_snapshot(
        &f.binding,
        "relay",
        ObservationSnapshot {
            users: None,
            actions: vec![],
            action_digest: format!("sha256:{:x}", Sha256::digest(b"[]")),
        },
    )
    .unwrap();
    assert!(result.gap.is_none(), "{:?}", result.gap);
    assert!(
        result
            .projection
            .unwrap()
            .rendered_prompt()
            .contains("exact  spacing")
    );
    assert_eq!(f.state().usage.calls, 0);
    assert!(
        f.observe(&["steer: Add a native instruction."])
            .gap
            .is_none()
    );
}

#[test]
fn forged_text_or_non_user_source_cannot_be_imported() {
    let mut quoted = quote("steer: Preserve BLUE.");
    quoted.text = "steer: Change to RED.".into();
    assert!(quoted.validate().is_err());
    let mut quoted = quote("steer: Preserve BLUE.");
    quoted.source.role = SourceRole::Delegate;
    assert!(quoted.validate().is_err());
}

#[test]
fn dispatch_copies_existing_intent_semantics_without_replacing_the_worker_goal() {
    use kah_core::intent_history::{IntentItem, IntentKind};
    let f = Fixture::new();
    let task = "steer: Implement the greeting.";
    f.observe(&[task]);
    let mut inherited = quote("Replace the earlier objective: validate BLUE before release.");
    let mut item = IntentItem::new(
        kah_observer::verbatim::item_id(&inherited.source),
        IntentKind::Validation,
        &inherited.text,
        vec![inherited.source.clone()],
    );
    item.review_required = true;
    item.kind_uncertain = true;
    inherited.dispatch_item = Some(item);
    let base = RunStore::default_state_root().unwrap();
    Store::accept_inherited(
        &base,
        &f.binding.session,
        SourceProvider::Bundle,
        inherited.clone(),
    )
    .unwrap();
    let result = f.observe(&[task]);
    assert!(result.gap.is_none(), "{:?}", result.gap);
    let projection = result.projection.unwrap();
    assert_eq!(projection.goal.text, task);
    let item = projection
        .validation
        .iter()
        .find(|i| i.text == inherited.text)
        .unwrap();
    assert!(item.review_required && item.kind_uncertain && !item.user_confirmed);
    assert_eq!(item.introduced_by, [inherited.source]);
    assert_eq!(f.state().usage.calls, 0);
}
#[test]
fn aside_survives_without_a_goal_and_user_can_promote_confirm_and_dismiss() {
    let f = Fixture::new();
    let aside = "aside:  Keep this exact note.\r\n";
    let first = f.observe(&[aside]);
    assert!(first.projection.is_none());
    assert!(first.gap.is_none(), "aside intake gap: {:?}", first.gap);
    let pending = review::list(&f.binding.session).unwrap();
    assert_eq!(pending["items"][0]["text"], aside);
    let id = pending["items"][0]["id"].as_str().unwrap();
    review::apply(&f.binding.session, "promote", id).unwrap();
    review::apply(&f.binding.session, "confirm", id).unwrap();
    let p = engine::projection(&f.state()).unwrap().unwrap();
    assert_eq!(p.frontier[0].text, aside);
    assert!(p.frontier[0].user_confirmed);
    review::apply(&f.binding.session, "dismiss", id).unwrap();
    assert!(
        engine::projection(&f.state())
            .unwrap()
            .unwrap()
            .frontier
            .is_empty()
    );
    assert_eq!(f.state().usage.calls, 0);
    let state = f.state();
    let store = Store::open(
        &RunStore::default_state_root().unwrap(),
        &state.binding.session,
        std::time::Duration::ZERO,
    )
    .unwrap();
    assert_eq!(store.sources().unwrap()[0].text, aside);
}
#[test]
fn forced_steering_is_durable_and_replay_does_not_reactivate_dismissed_input() {
    let f = Fixture::new();
    let text = "steer: Preserve BETA exactly";
    let first = f.observe(&[text]);
    assert!(first.gap.is_none(), "forced intake gap: {:?}", first.gap);
    let p = first.projection.unwrap();
    assert_eq!(p.goal.text, text);
    assert!(!p.needs_review());
    let id = p.frontier[0].id.to_string();
    review::apply(&f.binding.session, "dismiss", &id).unwrap();
    let replay = f.observe(&[text]);
    assert!(replay.projection.unwrap().frontier.is_empty());
    assert_eq!(f.state().usage.calls, 0);
}

#[test]
fn model_ignored_input_remains_retrievable_and_can_be_promoted_without_inference() {
    use kah_core::intent_history::SourceRef;
    use kah_observer::{
        intake::{Decision, Input},
        judge::{Answer, Judgment, Metrics},
    };
    let f = Fixture::new();
    let text = "Also preserve this user requirement";
    let record = BundleRecord::new("kah-user:fixture:0", SourceRole::User, text).unwrap();
    let mut store = Store::open(
        &RunStore::default_state_root().unwrap(),
        &f.binding.session,
        std::time::Duration::ZERO,
    )
    .unwrap();
    let input = Input {
        prior_intent: None,
        newer_conflicts: vec![],
        local_record: None,
        source: SourceRef::new(
            SourceProvider::Bundle,
            &f.binding.session,
            record.id(),
            SourceRole::User,
            record.content_digest(),
        ),
        text: text.into(),
        decision: Some(Decision::Jev {
            judgment: Box::new(Judgment {
                backend: "jev".into(),
                model: "fake-test".into(),
                answers: std::collections::BTreeMap::from([(
                    "is_steering".into(),
                    Answer::Noul { noul: 0.01 },
                )]),
                metrics: Metrics::default(),
                fallback_reason: None,
            }),
        }),
        dismissed: false,
        promoted: false,
    };
    assert!(input.recently_ignored());
    store.add_source(&input).unwrap();
    drop(store);
    assert!(f.observe(&[text]).projection.is_none());
    review::apply(&f.binding.session, "promote", &input.id()).unwrap();
    assert_eq!(
        engine::projection(&f.state()).unwrap().unwrap().frontier[0].text,
        text
    );
    let store = Store::open(
        &RunStore::default_state_root().unwrap(),
        &f.binding.session,
        std::time::Duration::ZERO,
    )
    .unwrap();
    assert!(!store.sources().unwrap()[0].recently_ignored());
    assert_eq!(f.state().usage.calls, 0);
}

#[test]
fn explicit_session_pause_prevents_source_reads_and_inference() {
    let f = Fixture::new();
    let base = RunStore::default_state_root().unwrap();
    drop(Store::open(&base, &f.binding.session, std::time::Duration::ZERO).unwrap());
    let marker = Store::directory(&base, &f.binding.session).join("observer-paused");
    fs::write(&marker, b"usage pause").unwrap();
    let result = engine::synchronize(&f.binding, "paused").unwrap();
    assert!(
        result.gap.is_none(),
        "paused observer tried its deliberately unavailable source"
    );
    assert!(result.verification.is_none() && result.reminder.is_none());
    assert!(kah_observer::session::paused(&base, &f.binding.session));
    fs::remove_file(marker).unwrap();
    assert!(!kah_observer::session::paused(&base, &f.binding.session));
}
