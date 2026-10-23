use kah_core::{
    intent_history::{History, SourceProvider, SourceRole},
    observation::ObservationSnapshot,
    run_store::{RuleSync, RunStore},
    session_bundle::{BundleRecord, NeutralSessionBundle},
};
use kah_observer::{
    engine, review,
    session::{Binding, State, Store},
};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

fn bundle(binding: &Binding, records: Vec<BundleRecord>) -> NeutralSessionBundle {
    NeutralSessionBundle::from_records(
        SourceProvider::Codex,
        &binding.session,
        format!(
            "sha256:{:x}",
            Sha256::digest(binding.project.as_os_str().as_encoded_bytes())
        ),
        records,
    )
    .unwrap()
}
fn snapshot(candidates: NeutralSessionBundle) -> ObservationSnapshot {
    let records = candidates
        .records()
        .iter()
        .filter(|r| r.is_authoritative())
        .cloned()
        .collect::<Vec<_>>();
    let users = (!records.is_empty()).then(|| {
        NeutralSessionBundle::from_records(
            candidates.source().provider(),
            candidates.source().session_ref_private(),
            candidates.source().repository_digest(),
            records,
        )
        .unwrap()
    });
    ObservationSnapshot {
        users,
        native_candidates: Some(candidates),
        actions: vec![],
        action_digest: format!("sha256:{:x}", Sha256::digest(b"[]")),
    }
}
fn isolated(probe: &str) {
    let root =
        std::env::temp_dir().join(format!("kah-classification-{}-{probe}", std::process::id()));
    fs::create_dir_all(&root).unwrap();
    let mut child = Command::new(std::env::current_exe().unwrap());
    child
        .args(["--exact", probe, "--nocapture"])
        .env("KAH_CLASSIFICATION_PROBE", &root)
        .env("XDG_STATE_HOME", root.join("state"))
        .env("HOME", root.join("home"))
        .env_remove("TYPESAFE_API_KEY")
        .env_remove("JEV_API_KEY");
    let output = child.output().unwrap();
    assert!(
        output.status.success(),
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    print!("{}", String::from_utf8_lossy(&output.stdout));
    fs::remove_dir_all(root).unwrap();
}
#[test]
fn historical_classification_survives_withdrawal_and_new_intake() {
    if std::env::var_os("KAH_CLASSIFICATION_PROBE").is_none() {
        return isolated("historical_classification_survives_withdrawal_and_new_intake");
    }
    let (binding, old) = legacy_fixture();
    let base = RunStore::default_state_root().unwrap();
    let input = Store::open_wait(&base, &binding.session)
        .unwrap()
        .sources()
        .unwrap()[1]
        .clone();
    let path = Store::directory(&base, &binding.session).join("observations.jsonl");
    let candidates = bundle(
        &binding,
        vec![
            old.records()[0].clone(),
            BundleRecord::new("u2", SourceRole::Delegate, input.text.clone()).unwrap(),
        ],
    );
    assert!(
        engine::synchronize_snapshot(&binding, "new-classifier", snapshot(candidates.clone()))
            .unwrap()
            .gap
            .is_none()
    );
    review::apply(&binding.session, "dismiss", &input.id()).unwrap();
    let mut records = candidates.records().to_vec();
    records
        .push(BundleRecord::new("u3", SourceRole::Delegate, "[kah-builder3] new context").unwrap());
    let outcome =
        engine::synchronize_snapshot(&binding, "next", snapshot(bundle(&binding, records)))
            .unwrap();
    assert!(outcome.gap.is_none(), "{:?}", outcome.gap);
    assert!(
        !outcome
            .projection
            .unwrap()
            .rendered_prompt()
            .contains("historical agent")
    );
    assert_eq!(
        Store::read(&base, &binding.session)
            .unwrap()
            .unwrap()
            .usage
            .calls,
        0
    );
    verify_classification_receipts(&binding, &path, &candidates);
}

fn legacy_fixture() -> (Binding, NeutralSessionBundle) {
    let root = PathBuf::from(std::env::var_os("KAH_CLASSIFICATION_PROBE").unwrap());
    let binding = Binding {
        session: "versioned-source".into(),
        project: root.join("project"),
        codex_home: root.join("codex"),
        native: "/nonexistent/no-provider".into(),
        model: "gpt-6.1-sol".into(),
        effort: "low".into(),
        timeout_secs: 1,
        judge: Default::default(),
        jev_model: "jev-1.13.0".into(),
    };
    fs::create_dir_all(&binding.project).unwrap();
    let old = bundle(
        &binding,
        vec![
            BundleRecord::new("u1", SourceRole::User, "steer: Keep intent.").unwrap(),
            BundleRecord::new(
                "u2",
                SourceRole::User,
                "steer: [kah-builder2] historical agent",
            )
            .unwrap(),
        ],
    );
    assert!(
        engine::synchronize_snapshot(&binding, "before", snapshot(old.clone()))
            .unwrap()
            .gap
            .is_none()
    );
    let base = RunStore::default_state_root().unwrap();
    remove_versions(&Store::directory(&base, &binding.session).join("observations.jsonl"));
    (binding, old)
}
fn remove_versions(path: &Path) {
    // Reproduce the pre-versioned journal: its stored User role is authoritative
    // for continuity, even when a later classifier changes that native role.
    let lines = fs::read_to_string(path)
        .unwrap()
        .lines()
        .filter_map(|l| {
            let mut v: Value = serde_json::from_str(l).unwrap();
            if v["kind"] == "source_classification_v1" {
                return None;
            }
            if v["kind"] == "user_source_v1" {
                v.as_object_mut().unwrap().remove("classifier_version");
            }
            Some(v.to_string())
        })
        .collect::<Vec<_>>()
        .join("\n")
        + "\n";
    fs::write(path, lines).unwrap();
}

#[test]
fn withdrawals_after_a_classifier_change_request_a_rebuild_without_source_rewrite() {
    if std::env::var_os("KAH_CLASSIFICATION_PROBE").is_none() {
        return isolated(
            "withdrawals_after_a_classifier_change_request_a_rebuild_without_source_rewrite",
        );
    }
    let root = PathBuf::from(std::env::var_os("KAH_CLASSIFICATION_PROBE").unwrap());
    let binding = Binding {
        session: "withdrawal-source".into(),
        project: root.join("withdrawal-project"),
        codex_home: root.join("codex"),
        native: "/nonexistent/no-provider".into(),
        model: "gpt-6.1-sol".into(),
        effort: "low".into(),
        timeout_secs: 1,
        judge: Default::default(),
        jev_model: "jev-1.13.0".into(),
    };
    fs::create_dir_all(&binding.project).unwrap();
    // A user goal and two delegate messages the old classifier stored as user authority.
    let texts = [
        "steer: Build the CSV export command with tests.",
        "steer: [kah-builder2] use the existing writer module",
        "steer: [kah-builder2] add a quoting helper",
    ];
    let old = texts
        .iter()
        .enumerate()
        .map(|(n, text)| BundleRecord::new(format!("w{n}"), SourceRole::User, *text).unwrap())
        .collect::<Vec<_>>();
    let outcome =
        engine::synchronize_snapshot(&binding, "before", snapshot(bundle(&binding, old.clone())))
            .unwrap();
    assert!(outcome.gap.is_none(), "{:?}", outcome.gap);
    let base = RunStore::default_state_root().unwrap();
    remove_versions(&Store::directory(&base, &binding.session).join("observations.jsonl"));
    let state = Store::read(&base, &binding.session).unwrap().unwrap();
    let run = state.run_id.clone().unwrap();
    let history = RunStore::open_default(&binding.project, &run)
        .unwrap()
        .recover()
        .unwrap()
        .history;
    let inputs = Store::open_wait(&base, &binding.session)
        .unwrap()
        .sources()
        .unwrap();
    assert_eq!(inputs.len(), texts.len());
    // The current classifier sees the two builder messages as delegate context.
    let candidates = old
        .iter()
        .map(|r| {
            let role = if r.content().contains("[kah-builder2]") {
                SourceRole::Delegate
            } else {
                SourceRole::User
            };
            BundleRecord::new(r.id(), role, r.content()).unwrap()
        })
        .collect();
    replay_next(&state, &run, &history, candidates, &inputs[1].id());
}
fn replay_next(
    state: &State,
    run: &str,
    history: &History,
    mut candidates: Vec<BundleRecord>,
    dismissed: &str,
) {
    let base = RunStore::default_state_root().unwrap();
    assert!(
        engine::synchronize_snapshot(
            &state.binding,
            "classifier-boundary",
            snapshot(bundle(&state.binding, candidates.clone()))
        )
        .unwrap()
        .gap
        .is_none()
    );
    review::apply(&state.binding.session, "dismiss", dismissed).unwrap();
    candidates.push(
        BundleRecord::new(
            "next-native-message",
            SourceRole::Delegate,
            "[kah-builder3] next context",
        )
        .unwrap(),
    );
    let usage = Store::read(&base, &state.binding.session)
        .unwrap()
        .unwrap()
        .usage
        .calls;
    let outcome = engine::synchronize_snapshot(
        &state.binding,
        "next-intake",
        snapshot(bundle(&state.binding, candidates)),
    )
    .unwrap();
    assert!(outcome.gap.is_none(), "{:?}", outcome.gap);
    verify_replay(state, run, history, dismissed, usage);
}
fn verify_replay(state: &State, run: &str, history: &History, dismissed: &str, usage: u64) {
    let base = RunStore::default_state_root().unwrap();
    let recovered = RunStore::open_default(&state.binding.project, run)
        .unwrap()
        .recover()
        .unwrap();
    let prefix = history.records().len();
    assert_eq!(&recovered.history.records()[..prefix], history.records());
    for (id, item) in history.intents() {
        let after = recovered.history.intent(id.as_str()).unwrap();
        if id.as_str() == dismissed {
            assert!(!after.is_active());
        } else {
            assert_eq!(after, item);
        }
    }
    assert_eq!(
        Store::read(&base, &state.binding.session)
            .unwrap()
            .unwrap()
            .usage
            .calls,
        usage
    );
    let rebuild = kah_observer::current_rules::stage(&state.binding)
        .unwrap()
        .unwrap();
    assert!(matches!(rebuild, RuleSync::Rebuild(_)));
}

fn verify_classification_receipts(
    binding: &Binding,
    path: &Path,
    candidates: &NeutralSessionBundle,
) {
    let journal = fs::read_to_string(path).unwrap();
    assert!(
        journal
            .lines()
            .map(|l| serde_json::from_str::<Value>(l).unwrap())
            .any(|v| v["kind"] == "source_classification_v1"
                && v["source"]["record"] == "u3"
                && v["classifier_version"] == 2
                && v["source"]["role"] == "delegate")
    );
    let base = RunStore::default_state_root().unwrap();
    let mut store = Store::open_wait(&base, &binding.session).unwrap();
    let mut records = candidates.records().to_vec();
    records.push(BundleRecord::new("u3", SourceRole::User, "[kah-builder3] new context").unwrap());
    assert_eq!(
        store
            .classified_users(&bundle(binding, records))
            .unwrap()
            .unwrap()
            .records()
            .len(),
        2
    );
    assert!(
        store
            .classified_users(&bundle(binding, vec![candidates.records()[0].clone()]))
            .unwrap_err()
            .contains("SOURCE_REWRITE")
    );
    let altered = BundleRecord::new("u2", SourceRole::Delegate, "changed original text").unwrap();
    assert!(
        store
            .classified_users(&bundle(
                binding,
                vec![candidates.records()[0].clone(), altered]
            ))
            .unwrap_err()
            .contains("SOURCE_REWRITE")
    );
}
