//! Report correction while host validation is still running.
use super::*;
fn fixture() -> (Store, PathBuf, Job) {
    let worker = format!("revision-{}", crate::relay_store::nonce());
    let root = directory(&worker, "review-1000000000012-1").unwrap();
    bind(&worker, &root);
    let store = Store::open(&worker).unwrap();
    kah_core::run_store::ensure_private_directory(&root).unwrap();
    let state = State::new(
        serde_json::from_value(
            json!({"provider":"codex","session":worker,"project":root,"pane":null,"socket":null}),
        )
        .unwrap(),
        &root,
    )
    .unwrap();
    let report: Report =
        serde_json::from_str(include_str!("../fixtures/report-revisions/old-report.json")).unwrap();
    let request: Request = serde_json::from_value(json!({"id":report.request_id,"worker":worker,"purpose":"checkpoint","turn":"retained-correction","head":null,"intent":{},"trigger":{},"documents":[],"created_ms":1791844015062u64})).unwrap();
    let submitted = root.join("submitted.json");
    fs::write(
        &submitted,
        include_bytes!("../fixtures/report-revisions/old-report.json"),
    )
    .unwrap();
    let job = Job {
        pending: Pending::from(&request),
        request,
        state,
        report,
        actions: vec![],
        submitted,
        report_revision: None,
        native_tokens_end: None,
        usage_gap: None,
    };
    (store, root, job)
}
#[test]
fn corrected_report_revision_replaces_immutable_old_job_without_host_changed_error() {
    let (store, root, old) = fixture();
    save_handoff_locked(&store, &root, old.clone()).unwrap();
    let mut latest = old.clone();
    latest.report =
        serde_json::from_str(include_str!("../fixtures/report-revisions/new-report.json")).unwrap();
    fs::write(
        &latest.submitted,
        include_bytes!("../fixtures/report-revisions/new-report.json"),
    )
    .unwrap();
    let result = save_handoff_locked(&store, &root, latest);
    assert!(
        result.is_ok(),
        "new validated report revision must replace old handoff: {result:?}"
    );
    let retained: Job = source_store::read(
        &root
            .join("revisions")
            .join(revision::key(&old).unwrap())
            .join("job.json"),
    )
    .unwrap();
    assert_eq!(
        serde_json::to_value(retained.report).unwrap(),
        serde_json::to_value(old.report).unwrap()
    );
    drop(store);
    fs::remove_dir_all(root.parent().unwrap().parent().unwrap()).unwrap();
}

fn bind(worker: &str, root: &Path) {
    let binding = kah_observer::session::Binding {
        session: worker.into(),
        project: root.into(),
        codex_home: root.into(),
        native: root.join("never-launched-native"),
        model: "gpt-6.1-sol".into(),
        effort: "low".into(),
        timeout_secs: 120,
        judge: Default::default(),
        jev_model: "jev-1.13.0".into(),
    };
    kah_observer::session::Store::open_wait(
        &kah_core::run_store::RunStore::default_state_root().unwrap(),
        worker,
    )
    .unwrap()
    .save(&kah_observer::session::State::new(binding))
    .unwrap();
}

#[test]
fn newer_bytes_during_validation_supersede_old_consume_and_preserve_both_snapshots() {
    let (store, root, mut old) = fixture();
    let original = fs::read(&old.submitted).unwrap();
    let snapshot = super::super::report_revision::Submission::capture(
        &store.directory,
        &old.submitted,
        &original,
    )
    .unwrap();
    old.report_revision = Some(snapshot.clone());
    fs::write(
        &old.submitted,
        include_bytes!("../fixtures/report-revisions/new-report.json"),
    )
    .unwrap();
    let error = save_handoff_locked(&store, &root, old.clone()).unwrap_err();
    assert!(
        error
            .to_string()
            .starts_with("REVIEW_REPORT_REVISION_SUPERSEDED")
    );
    assert!(super::super::failure::pending(error.as_ref()));
    assert!(!root.join("job.json").exists());
    assert_eq!(
        source_store::load_text(&snapshot.source)
            .unwrap()
            .as_bytes(),
        original
    );
    let newest = super::super::report_revision::Submission::capture(
        &store.directory,
        &old.submitted,
        &fs::read(&old.submitted).unwrap(),
    )
    .unwrap();
    assert_ne!(snapshot.digest, newest.digest);
    assert_eq!(
        source_store::load_text(&newest.source).unwrap().as_bytes(),
        include_bytes!("../fixtures/report-revisions/new-report.json")
    );
    drop(store);
    fs::remove_dir_all(root.parent().unwrap().parent().unwrap()).unwrap();
}

#[test]
fn request_identity_change_remains_fatal_when_report_revision_changes() {
    let (store, root, old) = fixture();
    save_handoff_locked(&store, &root, old.clone()).unwrap();
    let mut changed = old.clone();
    changed.request.turn = "another-request-binding".into();
    changed.report =
        serde_json::from_str(include_str!("../fixtures/report-revisions/new-report.json")).unwrap();
    fs::write(
        &changed.submitted,
        include_bytes!("../fixtures/report-revisions/new-report.json"),
    )
    .unwrap();
    let error = save_handoff_locked(&store, &root, changed).unwrap_err();
    assert!(error.to_string().starts_with("REVIEW_GATE_HOST_CHANGED"));
    assert!(!super::super::failure::pending(error.as_ref()));
    drop(store);
    fs::remove_dir_all(root.parent().unwrap().parent().unwrap()).unwrap();
}

#[test]
fn revised_gate_report_keeps_its_ack_after_a_newer_request_overwrites_ready() {
    let (store, root, mut old) = fixture();
    let mut state = acknowledged_gate(&store, &mut old);
    crate::unattended_report::save(
        &store.directory,
        "report-ready.json",
        &json!({"request":"review-later-normal-2916958"}),
    )
    .unwrap();
    fs::write(
        &old.submitted,
        include_bytes!("../fixtures/report-revisions/new-report.json"),
    )
    .unwrap();
    let report: Report = serde_json::from_slice(&fs::read(&old.submitted).unwrap()).unwrap();
    let revised = super::super::report_revision::Submission::capture(
        &store.directory,
        &old.submitted,
        &fs::read(&old.submitted).unwrap(),
    )
    .unwrap();
    state.pending = Some(old.pending.clone());
    let result = handoff_locked(
        &store,
        &mut state,
        &old.request,
        &report,
        &[],
        &revised,
        Ok(0),
    );
    assert!(
        result.is_ok(),
        "retained request ACK must survive overwrite: {result:?}"
    );
    assert!(state.pending.is_none());
    let ready: Value = super::super::state::read(&store.directory.join("report-ready.json"))
        .unwrap()
        .unwrap();
    assert_eq!(ready["request"], "review-later-normal-2916958");
    reject_changed_ack_bindings(&old, &revised);
    drop(store);
    fs::remove_dir_all(root.parent().unwrap().parent().unwrap()).unwrap();
}

fn acknowledged_gate(store: &Store, old: &mut Job) -> State {
    old.state.observer.session = Some("same-native-observer".into());
    old.state.pending = Some(old.pending.clone());
    store.save(&old.state).unwrap();
    crate::unattended_report::save(
        &store.directory,
        "report-ready.json",
        &json!({"request":old.request.id}),
    )
    .unwrap();
    // The real ACK includes the submitted path inside this observer's outbox.
    let submitted = old
        .state
        .observer
        .workspace
        .join("outbox")
        .join(format!("submitted-{}.json", old.request.id));
    kah_core::run_store::ensure_private_directory(submitted.parent().unwrap()).unwrap();
    fs::copy(&old.submitted, &submitted).unwrap();
    old.submitted = submitted;
    let initial = super::super::report_revision::Submission::capture(
        &store.directory,
        &old.submitted,
        &fs::read(&old.submitted).unwrap(),
    )
    .unwrap();
    let mut state = old.state.clone();
    assert!(
        handoff_locked(
            store,
            &mut state,
            &old.request,
            &old.report,
            &[],
            &initial,
            Ok(0)
        )
        .unwrap()
    );
    state
}

fn reject_changed_ack_bindings(old: &Job, revised: &super::super::report_revision::Submission) {
    let mut different = old.state.clone();
    different.observer.session = Some("different-native-observer".into());
    assert!(!revision::owns_ack(&different, &old.request, &revised.path).unwrap());
    let mut request = old.request.clone();
    request.turn = "changed-request-binding".into();
    assert!(!revision::owns_ack(&old.state, &request, &revised.path).unwrap());
    assert!(
        !revision::owns_ack(
            &old.state,
            &old.request,
            &revised.path.with_extension("wrong")
        )
        .unwrap()
    );
}
