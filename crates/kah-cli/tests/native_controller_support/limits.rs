use super::*;

pub fn run(root: &Path, agent: &str, mode: &str) -> Value {
    assert!(matches!(mode, "tokens" | "time" | "stop-verification"));
    let home = PathBuf::from(std::env::var_os("HOME").unwrap());
    let before = global_settings(&home);
    let project = fixture_project(
        &home,
        &format!("kah-native-run-{mode}-{agent}-{}", std::process::id()),
        Domain::Untrusted,
    );
    let options: &[&str] = match mode {
        "tokens" => &["--budget-tokens", "1"],
        "time" => &["--time-limit", "75"],
        _ => &[],
    };
    let verify = if mode == "tokens" {
        "true"
    } else {
        "test \"$(cat limit.txt)\" = READY && printf VERIFICATION_ACTIVE && sleep 120"
    };
    let task = "Create limit.txt containing READY with your native file tool and report. Do not push or deploy.";
    let output = launch(
        &home,
        &project.root,
        agent,
        fixture_model(agent),
        task,
        verify,
        options,
    );
    let id = output
        .lines()
        .find_map(|l| l.strip_prefix("Created "))
        .unwrap();
    fs::write(
        root.join("cleanup.json"),
        json!({"run":id,"source":project.root,"agent":agent}).to_string(),
    )
    .unwrap();
    let base = RunStore::default_state_root().unwrap().join("runs");
    let record = wait_limit(&base, id, agent, mode);
    let expected = check_limit(&base, id, mode, &record);
    for (path, bytes) in before {
        assert_eq!(bytes, fs::read(path).unwrap());
    }
    json!({"agent":agent,"version":version(&home,agent),"fixtureModel":fixture_model(agent),"fixtureEffort":"low","sourceDomain":"untrusted","consentRequired":true,"mode":mode,"status":expected,"tokens":record.control.tokens_used(),"elapsedMs":record.control.elapsed_ms(),"interruptedVerification":mode!="tokens","exhaustedResumeRefused":mode!="stop-verification","globalConfigUnchanged":true,"passed":true})
}

fn check_limit(base: &Path, id: &str, mode: &str, record: &Record) -> RunStatus {
    let expected = match mode {
        "tokens" => RunStatus::BudgetStopped,
        "time" => RunStatus::TimeStopped,
        _ => RunStatus::Stopped,
    };
    assert_eq!(record.published_status(), expected);
    let native = record.native.as_ref().unwrap();
    assert!(native.session.is_some());
    if mode == "tokens" {
        assert!(record.control.tokens_used() >= 1);
    }
    if mode == "time" {
        assert!(record.control.elapsed_ms() >= 75_000);
    }
    let directory = Store::path(base, id).unwrap();
    let report: Value =
        serde_json::from_slice(&fs::read(directory.join("report.json")).unwrap()).unwrap();
    if mode != "tokens" {
        assert_eq!(report["tests"]["success"], false);
        assert!(
            report["tests"]["output"]
                .as_str()
                .unwrap()
                .contains("RUN_TEST_INTERRUPTED")
        );
    }
    if mode != "stop-verification" {
        exhausted_resume(id, record);
    }
    expected
}

fn wait_limit(base: &Path, id: &str, agent: &str, mode: &str) -> Record {
    let deadline = Instant::now() + Duration::from_secs(180);
    let directory = Store::path(base, id).unwrap();
    let mut accepted = false;
    let mut started = false;
    let mut stop_sent = None;
    loop {
        let record = Store::read(base, id).unwrap();
        let native = record.native.as_ref().unwrap();
        if let Some(pane) = native.pane.as_ref() {
            let screen = pane_screen(pane, native.session.as_deref(), agent);
            if !accepted
                && native.waiting.is_some()
                && (screen.contains("Open restricted")
                    || screen.contains("Yes, I trust this folder"))
            {
                accept_consent(id, pane, agent);
                accepted = true;
            }
            started |= verification_started(&directory);
            if mode == "stop-verification" && started && stop_sent.is_none() {
                recovery::operator(&["stop", id]);
                stop_sent = Some(Instant::now());
            }
            if !matches!(
                record.published_status(),
                RunStatus::Running | RunStatus::Created | RunStatus::Stopping
            ) && directory.join("report.json").exists()
            {
                assert!(
                    mode == "tokens" || started,
                    "verification did not start before stop: {screen}"
                );
                assert!(
                    stop_sent.is_none_or(|t| t.elapsed() < Duration::from_secs(15)),
                    "stop did not interrupt verification promptly"
                );
                return record;
            }
            assert!(
                Instant::now() < deadline,
                "limit run timed out: {screen}; {}",
                serde_json::to_string(&record).unwrap()
            );
        }
        assert!(Instant::now() < deadline, "limit controller did not start");
        std::thread::sleep(Duration::from_millis(200));
    }
}
fn verification_started(directory: &Path) -> bool {
    fs::read_dir(directory).unwrap().flatten().any(|entry| {
        entry
            .file_name()
            .to_string_lossy()
            .starts_with("verification-")
            && fs::read_to_string(entry.path()).is_ok_and(|s| s.contains("VERIFICATION_ACTIVE"))
    })
}
fn exhausted_resume(id: &str, record: &Record) {
    let output = Command::new("kah").args(["resume", id]).output().unwrap();
    assert!(!output.status.success());
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("RUN_ALLOWANCE_EXHAUSTED"),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let current = Store::read(&RunStore::default_state_root().unwrap().join("runs"), id).unwrap();
    assert_eq!(current.control.tokens_used(), record.control.tokens_used());
    assert_eq!(current.control.elapsed_ms(), record.control.elapsed_ms());
    assert_eq!(
        current.native.as_ref().unwrap().pane,
        record.native.as_ref().unwrap().pane
    );
}
