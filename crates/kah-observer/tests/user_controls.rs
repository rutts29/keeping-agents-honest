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
            model: "gpt-6-luna".into(),
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
        engine::observe_snapshot(
            &self.binding,
            "fixture",
            false,
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
#[test]
fn aside_survives_without_a_goal_and_user_can_promote_confirm_and_dismiss() {
    let f = Fixture::new();
    let aside = "aside:  Keep this exact note.\r\n";
    let first = f.observe(&[aside]);
    assert!(first.projection.is_none());
    assert!(first.gap.is_none());
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
    let p = f.observe(&[text]).projection.unwrap();
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
