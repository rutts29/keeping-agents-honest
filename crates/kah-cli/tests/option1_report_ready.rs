//! Consume-before-delivery stays pending until this request is acknowledged.
use kah_cli::observer_review::{
    failure, option1,
    state::{Pending, Request, State, Store},
};
use kah_core::run_store::RunStore;
use kah_observer::session::{self, Binding};
use serde_json::json;
use std::{fs, path::PathBuf};
struct Fixture {
    worker: String,
    root: PathBuf,
    request: Request,
    state: State,
}
fn fixture() -> Fixture {
    let worker = format!("report-ready-{}", kah_cli::relay_store::nonce());
    let root = std::env::temp_dir().join(&worker);
    let binding = Binding {
        session: worker.clone(),
        project: root.clone(),
        codex_home: root.clone(),
        native: root.join("never-launched-native"),
        model: "gpt-6.1-sol".into(),
        effort: "low".into(),
        timeout_secs: 120,
        judge: Default::default(),
        jev_model: "jev-1.13.0".into(),
    };
    session::Store::open_wait(&RunStore::default_state_root().unwrap(), &worker)
        .unwrap()
        .save(&session::State::new(binding))
        .unwrap();
    let mut state = State::new(
        serde_json::from_value(
            json!({"provider":"codex","session":worker,"project":root,"pane":null,"socket":null}),
        )
        .unwrap(),
        &root,
    )
    .unwrap();
    state.observer.session = Some("same-native-observer".into());
    let request: Request = serde_json::from_value(json!({"id":"review-1000000-1","worker":worker,"purpose":"checkpoint","turn":"before-delivery","head":null,"intent":{},"trigger":{"option1":{"stage":"confirmation"}},"documents":[],"created_ms":1000050u64})).unwrap();
    state.pending = Some(Pending::from(&request));
    fs::create_dir_all(state.observer.workspace.join("outbox")).unwrap();
    Store::open(&worker).unwrap().save(&state).unwrap();
    Fixture {
        worker,
        root,
        request,
        state,
    }
}
fn consume(f: &Fixture) -> kah_cli::Result<()> {
    option1::consume::run(
        Store::open(&f.worker).unwrap(),
        f.state.clone(),
        f.request.clone(),
    )
}
#[test]
fn missing_report_and_stale_ack_are_pending_then_current_ack_allows_validation() {
    let f = fixture();
    let pending = consume(&f).unwrap_err();
    assert!(
        failure::pending(pending.as_ref()),
        "before delivery: {pending}"
    );
    let path = f
        .state
        .observer
        .workspace
        .join("outbox")
        .join(format!("submitted-{}.json", f.request.id));
    assert!(pending.to_string().contains(&path.display().to_string()));
    fs::write(
        &path,
        r#"{"request":"wrong-request","result":{"stage":"confirmation","rows":[]}}"#,
    )
    .unwrap();
    kah_cli::unattended_report::save(
        &Store::directory(&f.worker).unwrap(),
        "report-ready.json",
        &json!({"request":"old-review"}),
    )
    .unwrap();
    assert!(failure::pending(consume(&f).unwrap_err().as_ref()));
    kah_cli::unattended_report::save(
        &Store::directory(&f.worker).unwrap(),
        "report-ready.json",
        &json!({"request":f.request.id}),
    )
    .unwrap();
    let content = consume(&f).unwrap_err();
    assert!(content.to_string().starts_with("COMMIT_REPORT_BINDING"));
    assert_eq!(
        failure::origin(content.as_ref()),
        failure::Origin::ReportValidation
    );
    fs::remove_dir_all(f.root).unwrap();
}

#[test]
fn report_io_includes_exact_path_and_stage_and_cannot_be_report_correction() {
    let path =
        std::env::temp_dir().join(format!("missing-report-{}", kah_cli::relay_store::nonce()));
    let error = kah_cli::observer_review::consume::read_report(&path).unwrap_err();
    assert!(
        error
            .to_string()
            .starts_with("REVIEW_IO stage=report_open:")
    );
    assert!(error.to_string().contains(&path.display().to_string()));
    let wrapped =
        failure::validation::<()>(failure::Origin::ReportValidation, || Err(error)).unwrap_err();
    assert_eq!(failure::origin(wrapped.as_ref()), failure::Origin::Host);
}
