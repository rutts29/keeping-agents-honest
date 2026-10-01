use super::*;

pub fn run(root: &Path, agent: &str) -> Value {
    let home = PathBuf::from(std::env::var_os("HOME").unwrap());
    let before = global_settings(&home);
    let project = fixture_project(
        &home,
        &format!("kah-native-run-empty-{agent}-{}", std::process::id()),
        Domain::Untrusted,
    );
    let task = "Create empty-recovery.txt containing READY with your native file tool and report. Do not push or deploy.";
    let output = launch(
        &home,
        &project.root,
        agent,
        fixture_model(agent),
        task,
        "test -f empty-recovery.txt",
        &[],
    );
    let id = output
        .lines()
        .find_map(|s| s.strip_prefix("Created "))
        .unwrap();
    fs::write(
        root.join("cleanup.json"),
        json!({"run":id,"source":project.root,"agent":agent}).to_string(),
    )
    .unwrap();
    let base = RunStore::default_state_root().unwrap().join("runs");
    let initial = wait_consent(&base, id, agent, None);
    assert_eq!(initial.native.as_ref().unwrap().source_cursor, 0);
    assert!(!initial.worktree.join("empty-recovery.txt").exists());
    recovery::kill_controller(id, &initial);
    recovery::operator(&["resume", id]);
    let fresh = wait_consent(
        &base,
        id,
        agent,
        initial.native.as_ref().unwrap().pane.as_deref(),
    );
    let argv = recovery::native_argv(id, &fresh);
    assert!(
        !argv
            .iter()
            .any(|s| matches!(s.as_str(), "--resume" | "resume"))
    );
    assert!(argv.iter().any(|s| s == task));
    accept_consent(
        id,
        fresh.native.as_ref().unwrap().pane.as_ref().unwrap(),
        agent,
    );
    let done = wait_task(&base, id);
    assert!(done.native.as_ref().unwrap().session.is_some());
    assert_eq!(done.control.inputs().len(), 1);
    assert_eq!(done.control.task(), task);
    assert!(done.control.elapsed_ms() >= initial.control.elapsed_ms());
    assert!(!project.root.join("empty-recovery.txt").exists());
    for (path, bytes) in before {
        assert_eq!(bytes, fs::read(path).unwrap());
    }
    json!({"agent":agent,"version":version(&home,agent),"fixtureModel":fixture_model(agent),"fixtureEffort":"low","sourceDomain":"untrusted","consentRequired":true,"killedBeforeConsent":true,"freshLaunchInsteadOfMissingResume":true,"originalTaskPassedNatively":true,"newNativeSessionRecorded":true,"countersPreserved":true,"globalConfigUnchanged":true,"passed":true})
}
fn wait_consent(base: &Path, id: &str, agent: &str, old: Option<&str>) -> Record {
    let deadline = Instant::now() + Duration::from_secs(60);
    let mut zoomed = false;
    loop {
        let record = Store::read(base, id).unwrap();
        let n = record.native.as_ref().unwrap();
        if let Some(pane) = n.pane.as_deref().filter(|p| Some(*p) != old) {
            if !zoomed {
                recovery::operator_herdr(&["pane", "zoom", pane, "--on"]);
                zoomed = true;
            }
            let screen = pane_screen(pane, n.session.as_deref(), agent);
            if n.waiting.is_some()
                && (screen.contains("Open restricted")
                    || screen.contains("Yes, I trust this folder"))
            {
                return record;
            }
            assert!(
                Instant::now() < deadline,
                "empty journey did not reach consent: {screen}"
            );
        }
        assert!(Instant::now() < deadline, "empty controller not started");
        std::thread::sleep(Duration::from_millis(200));
    }
}
fn wait_task(base: &Path, id: &str) -> Record {
    let deadline = Instant::now() + Duration::from_secs(180);
    loop {
        let record = Store::read(base, id).unwrap();
        if record.worktree.join("empty-recovery.txt").exists()
            && record.native.as_ref().unwrap().synchronized()
        {
            recovery::operator(&["stop", id]);
            return recovery::wait_stopped(base, id);
        }
        assert!(
            Instant::now() < deadline,
            "fresh task did not run: {}",
            serde_json::to_string(&record).unwrap()
        );
        std::thread::sleep(Duration::from_millis(200));
    }
}
