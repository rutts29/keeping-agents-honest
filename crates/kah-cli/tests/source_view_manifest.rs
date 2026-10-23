//! 878-11: the observer refused a checkpoint because `read-source
//! manifest.json` failed with ENOENT. Its document blobs were in the private
//! store but never in the request's sandbox view.
use kah_cli::{
    observer_review::{
        redelivery, source_store,
        state::{Purpose, Request, State, Store},
        workspace,
    },
    relay_store::Endpoint,
};
use kah_observer::{review_evidence::Document, session};
use serde_json::Value;
use std::{
    fs,
    path::{Path, PathBuf},
    time::Duration,
};

#[test]
#[ignore = "host setup: sandbox service account and host ACL sharing"]
fn every_source_of_a_published_and_redelivered_view_resolves_in_the_view() {
    let worker = format!("manifest-view-unit-{}", std::process::id());
    // CAS sharing uses hard links; keep this workspace on the state filesystem.
    let home = kah_core::run_store::RunStore::default_state_root()
        .unwrap()
        .join(&worker);
    let state = setup(&worker, &home);
    fs::create_dir_all(home.join(".local/bin")).unwrap();
    fs::copy(env!("CARGO_BIN_EXE_kah"), home.join(".local/bin/kah")).unwrap();
    let request = Request {
        id: "review-manifest-unit".into(),
        worker: worker.clone(),
        purpose: Purpose::Drift,
        turn: "turn".into(),
        head: None,
        intent: serde_json::json!({"goal":"Whole user words."}),
        trigger: serde_json::json!({}),
        documents: vec![Document::new(
            "transcript",
            "worker_transcript",
            "native://transcript",
            &"Whole worker transcript line.\n".repeat(200),
        )],
        created_ms: 1,
    };
    let store = Store::open(&worker).unwrap();
    let input = state.observer.workspace.join("inputs").join(&request.id);
    workspace::prepare(&home, &state).unwrap();
    workspace::publish(&store, &state, &request).unwrap();
    assert_view_resolves(&store.directory, &input);
    // The 878-11 refusal came on the redelivered request (12:35:14).
    redelivery::prompt(&home, &store, &state, &request, &request.id).unwrap();
    assert_view_resolves(&store.directory, &input);
    let directory = store.directory.clone();
    drop(store);
    fs::remove_dir_all(directory.parent().unwrap()).unwrap();
    fs::remove_dir_all(home).unwrap();
}

/// The observer sandbox cannot reach the private store, only the view. Every
/// JSON file in the request input must expand from the view alone.
fn assert_view_resolves(private: &Path, input: &Path) {
    let hidden = private.join("sources-hidden");
    fs::rename(private.join("sources"), &hidden).unwrap();
    let mut failures = vec![];
    let mut checked = vec![];
    for entry in fs::read_dir(input).unwrap() {
        let path = entry.unwrap().path();
        if !path.is_file() || serde_json::from_slice::<Value>(&fs::read(&path).unwrap()).is_err() {
            continue;
        }
        checked.push(path.file_name().unwrap().to_string_lossy().into_owned());
        if let Err(error) = source_store::read::<Value>(&path) {
            failures.push(format!("{}: {error}", path.display()));
        }
    }
    fs::rename(&hidden, private.join("sources")).unwrap();
    assert!(checked.contains(&"manifest.json".to_owned()), "{checked:?}");
    assert!(failures.is_empty(), "{failures:#?}");
}

fn setup(worker: &str, home: &Path) -> State {
    let base = kah_core::run_store::RunStore::default_state_root().unwrap();
    let binding = session::Binding {
        session: worker.into(),
        project: home.into(),
        codex_home: home.join("codex"),
        native: PathBuf::from("/nonexistent/native"),
        model: "gpt-6.1-sol".into(),
        effort: "low".into(),
        timeout_secs: 90,
        judge: Default::default(),
        jev_model: "jev-latest".into(),
    };
    let mut primary = session::Store::open(&base, worker, Duration::ZERO).unwrap();
    primary.save(&session::State::new(binding)).unwrap();
    drop(primary);
    State::new(
        Endpoint {
            provider: "codex".into(),
            session: worker.into(),
            project: home.into(),
            pane: Some("unreachable-worker".into()),
            socket: None,
        },
        home,
    )
    .unwrap()
}
