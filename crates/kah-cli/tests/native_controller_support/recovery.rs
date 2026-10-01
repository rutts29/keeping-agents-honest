use super::*;

pub fn run(root: &Path, agent: &str) -> Value {
    let home = PathBuf::from(std::env::var_os("HOME").unwrap());
    let before = global_settings(&home);
    let domain = if agent == "codex" {
        Domain::Research
    } else {
        Domain::Mine
    };
    let project = fixture_project(
        &home,
        &format!("kah-native-run-recovery-{agent}-{}", std::process::id()),
        domain,
    );
    let task = "Create preserved.txt containing KEEP with your native file tool. Then run Bash/exec_command `printf ACTIVE > kill-ready.txt; sleep 60`. Afterward create resumed.txt containing DONE and report. If the session is interrupted and resumed, skip the fixture sleep and finish, preserving preserved.txt. Do not push or deploy.";
    let output = launch(
        &home,
        &project.root,
        agent,
        fixture_model(agent),
        task,
        "test \"$(cat preserved.txt)\" = KEEP && test -f resumed.txt",
        &[],
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
    let finished = recover(&base, id, agent, task);
    assert!(finished.native.as_ref().unwrap().synchronized());
    assert_eq!(
        fs::read_to_string(finished.worktree.join("preserved.txt"))
            .unwrap()
            .trim(),
        "KEEP"
    );
    assert!(!project.root.join("preserved.txt").exists());
    let retained = super::history_survival::prove(&home, root, id, &finished);
    for (path, bytes) in before {
        assert_eq!(bytes, fs::read(path).unwrap());
    }
    json!({"agent":agent,"version":version(&home,agent),"fixtureModel":fixture_model(agent),"fixtureEffort":"low","sourceDomain":domain.as_str(),"consentRequired":true,"controllerKilled":true,"nativeChildExited":true,"sameNativeSession":true,"nativeNamesVerified":true,"historyAfterCleanup":retained,"preservedPartialWrite":true,"resumedResult":if std::env::var_os("KAH_GATE_RECOVERY_STEER").is_some(){"BLUE"}else{"DONE"},"resumeHasNoModelEffortOverrides":true,"onlyOriginalUserIntent":std::env::var_os("KAH_GATE_RECOVERY_STEER").is_none(),"queuedCorrectionBeforeResume":std::env::var_os("KAH_GATE_RECOVERY_STEER").is_some(),"cumulativeTokens":finished.control.tokens_used(),"sourceUnchanged":true,"globalConfigUnchanged":true,"passed":true})
}

fn recover(base: &Path, id: &str, agent: &str, task: &str) -> Record {
    let original = wait_marker(base, id, agent, "kill-ready.txt", None);
    let home = PathBuf::from(std::env::var_os("HOME").unwrap());
    proof::native_name(&home, &Store::path(base, id).unwrap(), id, &original);
    let original_bytes = original
        .native
        .as_ref()
        .unwrap()
        .confirmation
        .as_ref()
        .map(|c| fs::metadata(&c.transcript).unwrap().len());
    assert!(!original.worktree.join("resumed.txt").exists());
    assert!(
        fs::metadata(original.worktree.join("kill-ready.txt"))
            .unwrap()
            .modified()
            .unwrap()
            .elapsed()
            .unwrap()
            < Duration::from_secs(20)
    );
    kill_controller(id, &original);
    let correction=std::env::var_os("KAH_GATE_RECOVERY_STEER").map(|_| "steer: Change the final contents of resumed.txt to BLUE instead of DONE. Preserve preserved.txt. Skip the fixture sleep after interruption and finish.");
    if let Some(text) = correction {
        operator(&["steer", id, text]);
    }
    operator(&["resume", id]);
    let old_pane = original.native.as_ref().unwrap().pane.as_deref();
    let resumed = wait_marker(base, id, agent, "resumed.txt", old_pane);
    proof::native_name(&home, &Store::path(base, id).unwrap(), id, &resumed);
    check_resumed(id, &original, &resumed, original_bytes, task, correction);
    operator(&["stop", id]);
    let finished = wait_stopped(base, id);
    assert!(finished.control.tokens_used() >= original.control.tokens_used());
    finished
}

fn check_resumed(
    id: &str,
    original: &Record,
    resumed: &Record,
    original_bytes: Option<u64>,
    task: &str,
    correction: Option<&str>,
) {
    if let Some(bytes) = original_bytes {
        let confirmation = resumed
            .native
            .as_ref()
            .unwrap()
            .confirmation
            .as_ref()
            .unwrap();
        assert_eq!(confirmation.source, "resume");
        assert!(fs::metadata(&confirmation.transcript).unwrap().len() > bytes);
    }
    assert_eq!(
        original.native.as_ref().unwrap().session,
        resumed.native.as_ref().unwrap().session
    );
    assert_eq!(
        resumed.control.inputs().len(),
        if correction.is_some() { 2 } else { 1 },
        "automation must not become user intent"
    );
    assert_eq!(resumed.control.task(), task);
    if let Some(text) = correction {
        assert_eq!(resumed.control.inputs()[1].text, text);
    }
    assert_eq!(
        fs::read_to_string(resumed.worktree.join("resumed.txt"))
            .unwrap()
            .trim(),
        if correction.is_some() { "BLUE" } else { "DONE" }
    );
    let argv = native_argv(id, resumed);
    assert!(
        argv.iter()
            .any(|a| matches!(a.as_str(), "resume" | "--resume"))
    );
    assert!(
        !argv
            .iter()
            .any(|a| matches!(a.as_str(), "--model" | "--effort"))
    );
}

fn wait_marker(base: &Path, id: &str, agent: &str, marker: &str, old_pane: Option<&str>) -> Record {
    let deadline = Instant::now() + Duration::from_secs(240);
    let mut accepted = false;
    loop {
        let record = Store::read(base, id).unwrap();
        let native = record.native.as_ref().unwrap();
        if let Some(pane) = native.pane.as_deref().filter(|p| Some(*p) != old_pane) {
            let screen = pane_screen(pane, native.session.as_deref(), agent);
            if !accepted
                && native.waiting.is_some()
                && (screen.contains("Open restricted")
                    || screen.contains("Yes, I trust this folder"))
            {
                accept_consent(id, pane, agent);
                accepted = true;
            }
            if record.worktree.join(marker).exists()
                && native.session.is_some()
                && (marker != "resumed.txt" || native.synchronized())
            {
                return record;
            }
            if !matches!(
                record.published_status(),
                RunStatus::Running | RunStatus::Created | RunStatus::Stopping
            ) && Store::path(base, id).unwrap().join("report.json").exists()
            {
                eprintln!(
                    "recovery report: {}",
                    fs::read_to_string(Store::path(base, id).unwrap().join("report.json")).unwrap()
                );
                panic!(
                    "native recovery stopped: {screen}; {}",
                    serde_json::to_string(&record).unwrap()
                );
            }
            assert!(
                Instant::now() < deadline,
                "native recovery timed out: {screen}; {}",
                serde_json::to_string(&record).unwrap()
            );
        }
        assert!(
            Instant::now() < deadline,
            "recovery controller did not start"
        );
        std::thread::sleep(Duration::from_millis(200));
    }
}
pub(super) fn controller(id: &str, record: &Record) -> u32 {
    let matches = crate::processes::list()
        .into_iter()
        .filter(|p| {
            p.state != 'Z'
                && p.cwd == record.worktree.to_string_lossy()
                && argv(p.pid).get(1..3) == Some(&["__native-run".into(), id.into()])
        })
        .collect::<Vec<_>>();
    assert_eq!(
        matches.len(),
        1,
        "must identify exactly the fixture controller"
    );
    matches[0].pid
}
pub(super) fn native_pid(parent: u32, agent: &str) -> u32 {
    let matches = crate::processes::list()
        .into_iter()
        .filter(|p| p.parent == parent && p.state != 'Z')
        .filter(|p| {
            argv(p.pid)
                .first()
                .is_some_and(|a| Path::new(a).file_name().is_some_and(|n| n == agent))
                && !argv(p.pid).iter().any(|a| a == "app-server")
        })
        .collect::<Vec<_>>();
    assert_eq!(matches.len(), 1, "must identify the native child");
    matches[0].pid
}
pub(super) fn native_argv(id: &str, record: &Record) -> Vec<String> {
    argv(native_pid(
        controller(id, record),
        &record.native.as_ref().unwrap().provider,
    ))
}
fn argv(pid: u32) -> Vec<String> {
    fs::read(format!("/proc/{pid}/cmdline"))
        .unwrap_or_default()
        .split(|b| *b == 0)
        .filter(|b| !b.is_empty())
        .map(|s| String::from_utf8_lossy(s).into_owned())
        .collect()
}
pub(super) fn kill_controller(id: &str, record: &Record) {
    let parent = controller(id, record);
    let child = native_pid(parent, &record.native.as_ref().unwrap().provider);
    let stamp = fs::read_to_string(format!("/proc/{parent}/stat"))
        .unwrap()
        .rsplit_once(") ")
        .unwrap()
        .1
        .split_whitespace()
        .nth(19)
        .unwrap()
        .to_owned();
    assert_eq!(controller(id, record), parent);
    assert_eq!(
        fs::read_to_string(format!("/proc/{parent}/stat"))
            .unwrap()
            .rsplit_once(") ")
            .unwrap()
            .1
            .split_whitespace()
            .nth(19)
            .unwrap(),
        stamp
    );
    assert!(
        Command::new("/bin/kill")
            .args(["-KILL", &parent.to_string()])
            .status()
            .unwrap()
            .success()
    );
    let deadline = Instant::now() + Duration::from_secs(10);
    while crate::processes::list()
        .iter()
        .any(|p| p.pid == child && p.state != 'Z')
    {
        assert!(
            Instant::now() < deadline,
            "native child outlived killed controller"
        );
        std::thread::sleep(Duration::from_millis(100));
    }
}
pub(super) fn operator(args: &[&str]) {
    let output = Command::new("kah").args(args).output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}
pub(super) fn operator_herdr(args: &[&str]) {
    let output = Command::new("herdr").args(args).output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}
pub(super) fn wait_stopped(base: &Path, id: &str) -> Record {
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        let record = Store::read(base, id).unwrap();
        if matches!(
            record.published_status(),
            RunStatus::Stopped | RunStatus::Completed
        ) && Store::path(base, id).unwrap().join("report.json").exists()
        {
            return record;
        }
        assert!(Instant::now() < deadline, "stop did not finish");
        std::thread::sleep(Duration::from_millis(200));
    }
}
