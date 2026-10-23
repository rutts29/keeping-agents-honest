use super::*;
pub fn run_dispatch(root: &Path, agent: &str, parent: &str) -> Value {
    let home = PathBuf::from(std::env::var_os("HOME").unwrap());
    let project = fixture_project(
        &home,
        &format!("kah-native-run-inherited-{agent}-{}", std::process::id()),
        Domain::Mine,
    );
    let task = "steer: Create marker.txt with the exact content required by the inherited user instruction. Do not create other files.";
    let output = launch(
        &home,
        &project.root,
        agent,
        fixture_model(agent),
        task,
        "test \"$(cat marker.txt)\" = BLUE && test \"$(wc -c < marker.txt)\" -eq 5 && test ! -e forbidden.txt",
        &["--parent", parent],
    );
    let id = output
        .lines()
        .find_map(|line| line.strip_prefix("Created "))
        .unwrap();
    fs::write(
        root.join("cleanup.json"),
        json!({"run":id,"source":project.root,"agent":agent}).to_string(),
    )
    .unwrap();
    let base = RunStore::default_state_root().unwrap().join("runs");
    let pane = launched_pane(&output, id);
    zoom(&base, id, pane);
    let record = wait(&base, id, agent);
    assert_eq!(record.native.as_ref().unwrap().pane.as_deref(), Some(pane));
    let report = diagnostics(&Store::path(&base, id).unwrap(), &record.worktree);
    let dispatch: Value = serde_json::from_slice(
        &fs::read(Store::path(&base, id).unwrap().join("dispatch.json")).unwrap(),
    )
    .unwrap();
    assert!(
        dispatch["relevance"].is_null(),
        "the fixture's constraints must not be relevance-scored"
    );
    assert_eq!(dispatch["relevance_sources"], json!([]));
    assert_eq!(record.published_status(), RunStatus::Completed, "{report}");
    let count = prove(&record, task, parent);
    let result = json!({"agent":agent,"version":version(&home, agent),"status":"completed","inheritedItems":count,"constraintsBypassedRelevance":true,"originalParentSource":true,"importedBeforeFirstWrite":true,"taskGoalPreserved":true,"testsPassed":report["tests"]["success"],"passed":true});
    assert_eq!(
        record.native.as_ref().unwrap().herdr_socket,
        std::env::var_os("HERDR_SOCKET_PATH").map(PathBuf::from)
    );
    kah_cli::run_pane::Pane {
        id: record.native.as_ref().unwrap().pane.clone().unwrap(),
    }
    .close()
    .unwrap();
    cleanup(root);
    fs::remove_file(root.join("cleanup.json")).unwrap();
    result
}
/// The pane that `kah run` started and printed ("Run <id>; native pane <pane>").
/// The run store gets it only after the worker in that pane has built its
/// sandbox, which can take time under load.
fn launched_pane<'a>(output: &'a str, id: &str) -> &'a str {
    let prefix = format!("Run {id}; native pane ");
    output
        .lines()
        .find_map(|line| line.strip_prefix(&prefix)?.split(';').next())
        .unwrap_or_else(|| panic!("kah run printed no native pane: {output}"))
}
fn zoom(base: &Path, id: &str, pane: &str) {
    assert!(
        Command::new("herdr")
            .args(["pane", "zoom", pane, "--on"])
            .output()
            .unwrap()
            .status
            .success()
    );
    // Cross-check only: the run store may not have the pane yet.
    if let Some(recorded) = Store::read(base, id).unwrap().native.unwrap().pane {
        assert_eq!(recorded, pane);
    }
}
fn prove(record: &Record, task: &str, parent: &str) -> usize {
    let session = record.native.as_ref().unwrap().session.as_ref().unwrap();
    let received = kah_cli::relay_store::directory(session)
        .unwrap()
        .join("dispatch-received.json");
    assert!(
        fs::metadata(&received).unwrap().modified().unwrap()
            <= fs::metadata(record.worktree.join("marker.txt"))
                .unwrap()
                .modified()
                .unwrap()
    );
    let observer = kah_observer::session::Store::open(
        &RunStore::default_state_root().unwrap(),
        session,
        Duration::from_secs(2),
    )
    .unwrap();
    let sources = observer.sources().unwrap();
    assert_eq!(sources[0].text, task);
    let inherited = sources
        .iter()
        .filter(|i| i.local_record.is_some())
        .collect::<Vec<_>>();
    assert!(!inherited.is_empty());
    assert!(
        inherited
            .iter()
            .any(|i| i.text.contains("BLUE") && i.source.session_private() == parent)
    );
    inherited.len()
}
