use kah_core::{
    intent_history::{SourceProvider, SourceRole},
    observation::ObservationSnapshot,
    session_bundle::{BundleRecord, NeutralSessionBundle},
};
use kah_observer::{engine, judge::routing::Backend, session::Binding};
use std::{fs, path::PathBuf};
#[test]
fn supplied_worker_evidence_cannot_be_attached_to_another_session() {
    let project = std::env::temp_dir().join(format!("kah-snapshot-binding-{}", std::process::id()));
    fs::create_dir_all(&project).unwrap();
    let binding = Binding {
        session: "run-expected".into(),
        project: project.clone(),
        codex_home: PathBuf::from("/unused"),
        native: PathBuf::from("/unused"),
        model: "gpt-6-luna".into(),
        effort: "low".into(),
        timeout_secs: 1,
        judge: Backend::Jev,
        jev_model: "jev-latest".into(),
    };
    let users = NeutralSessionBundle::from_records(
        SourceProvider::Bundle,
        "run-other",
        format!("sha256:{}", "a".repeat(64)),
        vec![BundleRecord::new("u0", SourceRole::User, "task").unwrap()],
    )
    .unwrap();
    let result = engine::observe_snapshot(
        &binding,
        "turn",
        true,
        ObservationSnapshot {
            users: Some(users),
            actions: vec![],
            action_digest: String::new(),
        },
    );
    fs::remove_dir_all(project).unwrap();
    assert!(result.err().unwrap().starts_with("OBSERVER_SOURCE_SESSION"));
}
