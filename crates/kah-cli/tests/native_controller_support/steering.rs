use super::*;

const CORRECTION: &str = "steer: Change choice.txt to BLUE instead of RED, using your native file edit tool. Preserve waiting.txt. Report the revised choice and stop.";

pub fn run(root: &Path, agent: &str) -> Value {
    let home = PathBuf::from(std::env::var_os("HOME").unwrap());
    let before = [
        home.join(".codex/config.toml"),
        home.join(".claude/settings.json"),
    ]
    .map(|p| (p.clone(), fs::read(p).unwrap()));
    let domain = if agent == "claude" {
        Domain::Untrusted
    } else {
        Domain::Mine
    };
    let name = format!("kah-native-run-steering-{agent}-{}", std::process::id());
    let project = Project::select(&home.join("repos"), &home, Some((domain, &name))).unwrap();
    assert!(project.needs_creation);
    project.initialize().unwrap();
    let (file, shell) = if agent == "claude" {
        ("Write", "Bash")
    } else {
        ("apply_patch", "exec_command")
    };
    let task = format!(
        "In this disposable worktree, use {file} to create choice.txt containing RED. Then use {shell} to run `printf WAITING > waiting.txt; sleep 30`. This wait lets the human send a correction while you work. Afterward report the choice. Do not push or deploy."
    );
    let output = launch(
        &home,
        &project.root,
        agent,
        fixture_model(agent),
        &task,
        "test \"$(cat choice.txt)\" = BLUE",
        &[],
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
    let record = steer_and_wait(&base, id, agent);
    assert_eq!(record.control.inputs().len(), 2);
    assert_eq!(record.control.inputs()[0].text, task);
    assert_eq!(record.control.inputs()[1].text, CORRECTION);
    assert_eq!(
        fs::read_to_string(record.worktree.join("choice.txt"))
            .unwrap()
            .trim(),
        "BLUE"
    );
    assert!(!project.root.join("choice.txt").exists());
    assert_eq!(record.native.as_ref().unwrap().source_cursor, 2);
    assert!(record.native.as_ref().unwrap().synchronized());
    for (path, bytes) in before {
        assert_eq!(bytes, fs::read(path).unwrap());
    }
    json!({"agent":agent,"version":version(&home,agent),"fixtureModel":fixture_model(agent),"fixtureEffort":"low","sourceDomain":domain.as_str(),"consentRequired":agent=="claude","codexMineNoConsentKeys":agent=="codex","steeredDuringNativeShell":true,"exactPrimaryInputs":true,"revisedChoice":"BLUE","sourceUnchanged":true,"status":record.published_status(),"globalConfigUnchanged":true,"passed":true})
}

fn steer_and_wait(base: &Path, id: &str, agent: &str) -> Record {
    let deadline = Instant::now() + Duration::from_secs(240);
    let mut accepted = false;
    let mut injected = false;
    let mut stopped = false;
    loop {
        let record = Store::read(base, id).unwrap();
        let native = record.native.as_ref().unwrap();
        if let Some(pane) = native.pane.as_ref() {
            let screen = pane_screen(pane, native.session.as_deref(), agent);
            if agent == "claude"
                && !accepted
                && native.waiting.is_some()
                && screen.contains("Yes, I trust this folder")
            {
                accept_consent(id, pane, agent);
                accepted = true;
            }
            if !injected && record.worktree.join("waiting.txt").exists() {
                inject_during_sleep(base, id, &record.worktree);
                injected = true;
            }
            if injected
                && !stopped
                && native.synchronized()
                && native.source_cursor == 2
                && fs::read_to_string(record.worktree.join("choice.txt"))
                    .is_ok_and(|s| s.trim() == "BLUE")
            {
                operator(&["stop", id]);
                stopped = true;
            }
            if !matches!(
                record.published_status(),
                RunStatus::Created | RunStatus::Running | RunStatus::Stopping
            ) && Store::path(base, id).unwrap().join("report.json").exists()
            {
                assert!(
                    injected && stopped,
                    "run stopped before steering was followed: {screen}"
                );
                return record;
            }
            assert!(
                Instant::now() < deadline,
                "steering timed out: {screen}; {}",
                serde_json::to_string(&record).unwrap()
            );
        }
        assert!(
            Instant::now() < deadline,
            "controller did not register its pane"
        );
        std::thread::sleep(Duration::from_millis(200));
    }
}
fn inject_during_sleep(base: &Path, id: &str, worktree: &Path) {
    assert_eq!(
        fs::read_to_string(worktree.join("choice.txt"))
            .unwrap()
            .trim(),
        "RED"
    );
    let marker = fs::metadata(worktree.join("waiting.txt"))
        .unwrap()
        .modified()
        .unwrap();
    operator(&["steer", id, CORRECTION]);
    loop {
        let record = Store::read(base, id).unwrap();
        let native = record.native.as_ref().unwrap();
        assert!(
            marker.elapsed().unwrap() < Duration::from_secs(25),
            "correction was not delivered during native sleep"
        );
        if native.submitted.contains(&1) || native.source_cursor >= 2 {
            return;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
}
fn operator(args: &[&str]) {
    let result = Command::new("kah").args(args).output().unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
}
