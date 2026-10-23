//! An unconfirmed tag verdict covers the run-end event on the same head.
#[allow(dead_code)]
mod checkpoint_replay;
use kah_cli::observer_review::{
    option1,
    state::{State, Store},
};
use serde_json::json;
use std::fs;
use std::os::unix::fs::PermissionsExt;

#[test]
fn same_sha_completion_carries_unconfirmed_tag_without_new_decisions() {
    std::env::var_os("XDG_STATE_HOME").expect("run through demos/check-tests.sh");
    let worker = format!("checkpoint-carry-{}", kah_cli::relay_store::nonce());
    let root = std::env::temp_dir().join(&worker);
    fs::create_dir_all(&root).unwrap();
    let store = Store::open(&worker).unwrap();
    let directory = store.directory.clone();
    fs::set_permissions(
        directory.parent().unwrap(),
        fs::Permissions::from_mode(0o700),
    )
    .unwrap();
    let tag = &checkpoint_replay::tag();
    let checkpoint = json!({"head":checkpoint_replay::HEAD,"intent_revision":2,"request":tag["id"],
        "passed":false,"result":tag["latest"]});
    let state: State = serde_json::from_value(json!({
        "worker":{"provider":"claude","session":worker,"project":root,"pane":null,"socket":null},
        "observer":{"session":"recorded-observer","pane":null,"name":"recorded-observer","workspace":root,
            "model":"claude-sonnet-5-5","effort":"low","transcript":null},
        "pending":null,"latest":tag["latest"],"last_verified_turn":null,"checkpoint":checkpoint,
        "deferrals":[],"wakes":0,"native_tokens":0,"stopped":false})).unwrap();
    install_history(&root, &worker);
    store.save(&state).unwrap();
    let mut trigger = checkpoint_replay::completion_trigger();
    trigger["head"] = checkpoint_replay::HEAD.into();
    let result = option1::admission::submit(store, state, &root, "recorded-end", trigger).unwrap();
    assert_eq!(result["status"], "option1_reviewed");
    assert_eq!(result["carried_from"], tag["id"]);
    let saved = Store::read(&worker).unwrap().unwrap();
    assert!(saved.pending.is_none());
    assert_eq!(saved.checkpoint.as_ref().unwrap()["passed"], false);
    assert_eq!(saved.checkpoint.as_ref().unwrap()["result"], tag["latest"]);
    assert_eq!(saved.wakes, 0);
    assert!(!directory.join("requests").exists());
    fs::remove_dir_all(directory.parent().unwrap()).unwrap();
    fs::remove_dir_all(root).unwrap();
}

fn install_history(project: &std::path::Path, worker: &str) {
    use kah_core::{
        projection::{ActiveProjection, ProjectionConfig},
        run_store::RunStore,
    };
    use kah_observer::session::{Binding, State as IntakeState, Store as IntakeStore};
    let history = checkpoint_replay::history();
    let projection = ActiveProjection::from_history(&history, ProjectionConfig::default()).unwrap();
    let base = RunStore::default_state_root().unwrap();
    drop(RunStore::create(&base, project, "run-same-sha-carry", &history, &projection).unwrap());
    let binding = Binding {
        session: worker.into(),
        project: project.into(),
        codex_home: "/unused".into(),
        native: "/nonexistent/no-provider".into(),
        model: "claude-sonnet-5-5".into(),
        effort: "low".into(),
        timeout_secs: 1,
        judge: Default::default(),
        jev_model: "jev-1.13.0".into(),
    };
    let mut state = IntakeState::new(binding);
    state.run_id = Some("run-same-sha-carry".into());
    IntakeStore::open_wait(&base, worker)
        .unwrap()
        .save(&state)
        .unwrap();
}
