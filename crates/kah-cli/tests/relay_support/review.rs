use super::*;
use kah_cli::observer_review::{
    runtime,
    state::{Purpose, Store as Reviews},
};

pub fn run(root: &Path) -> Value {
    assert!(
        std::env::var("HERDR_SOCKET_PATH")
            .unwrap()
            .contains("kah-gate-")
    );
    let home = PathBuf::from(std::env::var_os("HOME").unwrap());
    let configs = [
        home.join(".codex/config.toml"),
        home.join(".claude/settings.json"),
    ]
    .map(|p| (p.clone(), fs::read(p).unwrap()));
    let mut results = vec![];
    for provider in ["codex", "claude"] {
        if std::env::var("KAH_GATE_REVIEW_PROVIDER").is_ok_and(|selected| selected != provider) {
            continue;
        }
        let (launch, mut codex) = crate::fixture::prepare(&home, "research", false);
        let claude = (provider == "claude").then(crate::claude_support::Fixture::new);
        let pane = if provider == "codex" {
            let mut command = launch.command(false);
            command
                .args(["--no-alt-screen", BLUE])
                .env_remove("CODEX_THREAD_ID");
            Pane::start(&command).unwrap()
        } else {
            start_claude(&home, claude.as_ref().unwrap(), root)
        };
        let mut panes = Panes(vec![pane]);
        let id = ready(&panes.0[0], provider);
        if provider == "codex" {
            codex.sessions.push(id.clone());
        }
        let cleanup = Cleanup {
            home: home.clone(),
            worker: id.clone(),
        };
        let mut result = wakes(&id, provider, &mut panes);
        if std::env::var_os("KAH_GATE_REVIEW_LIVENESS").is_some() {
            result["stallRecovery"] = super::review_liveness::run(&id, provider, &mut panes);
            fs::write(
                PathBuf::from(std::env::var_os("KAH_GATE_EVIDENCE").unwrap())
                    .join(format!("liveness-{provider}.json")),
                serde_json::to_vec_pretty(&result["stallRecovery"]).unwrap(),
            )
            .unwrap();
        }
        result["lifecycle"] = lifecycle(&home, &id, provider, &mut panes);
        results.push(result);
        drop(panes);
        drop(cleanup);
    }
    for (path, before) in configs {
        assert!(
            fs::read(path).unwrap() == before,
            "global provider config changed"
        );
    }
    json!({"versions":versions(&home),"fixtureModels":{"codex":"gpt-6.1-sol/low","claude":"claude-sonnet-5-5/low"},"persistentPaneHandoff":results,"catchingProof":false,"passed":true})
}
fn wakes(id: &str, provider: &str, panes: &mut Panes) -> Value {
    let mut observer_id = None;
    let mut observer_pane = None;
    let mut wakes = vec![];
    let rounds = std::env::var("KAH_GATE_REVIEW_ROUNDS")
        .map(|n| n.parse::<usize>().unwrap())
        .unwrap_or(3);
    assert!((1..=3).contains(&rounds));
    for number in 1..=rounds {
        let result = round(id, number);
        let result = await_submission(id, provider, panes, result, &format!("fixture-{number}"));
        let pane = Pane {
            id: result["pane"].as_str().unwrap().into(),
        };
        if observer_pane.is_none() {
            assert_eq!(
                pane.inspect().unwrap()["focused"],
                false,
                "observer stole focus"
            );
            panes.0.push(pane.clone());
            assert!(
                Command::new("herdr")
                    .args(["pane", "zoom", &pane.id, "--on"])
                    .output()
                    .unwrap()
                    .status
                    .success()
            );
        }
        let review = wait_review(id, &pane, provider);
        assert!(review.latest.is_some());
        if let Some(expected) = &observer_id {
            assert_eq!(review.observer.session.as_ref(), Some(expected));
        }
        if let Some(expected) = &observer_pane {
            assert_eq!(review.observer.pane.as_ref(), Some(expected));
        }
        observer_id = review.observer.session.clone();
        observer_pane = review.observer.pane.clone();
        let result = review.latest.unwrap();
        assert!(
            result["native_tokens"].as_u64().is_some_and(|n| n > 0),
            "missing native per-wake usage: {result}"
        );
        fs::write(
            PathBuf::from(std::env::var_os("KAH_GATE_EVIDENCE").unwrap())
                .join(format!("persistent-{provider}-wake-{number}.json")),
            serde_json::to_vec_pretty(&result).unwrap(),
        )
        .unwrap();
        wakes.push(result);
    }
    json!({"provider":provider,"observerSession":observer_id,"observerPane":observer_pane,"wakes":wakes})
}
fn round(id: &str, number: usize) -> Value {
    retained(
        runtime::wake(
            id,
            Purpose::Drift,
            &format!("fixture-{number}"),
            json!({"fixture":"persistent pane handoff; not a catching proof","check":number}),
        ),
        id,
    )
}
fn retained(result: kah_cli::Result<Value>, worker: &str) -> Value {
    match result {
        Ok(value) => value,
        Err(error)
            if kah_cli::observer_review::redelivery::retryable(
                kah_cli::observer_review::failure::origin(error.as_ref()),
                &error.to_string(),
            ) =>
        {
            let state = Reviews::read(worker).unwrap().unwrap();
            json!({"status":"retained_delivery_gap","pane":state.observer.pane,"request":state.pending.unwrap().id,"gap":error.to_string()})
        }
        Err(error) => panic!("{error}"),
    }
}
fn await_submission(
    id: &str,
    provider: &str,
    panes: &mut Panes,
    mut result: Value,
    turn: &str,
) -> Value {
    while result["status"] == "queued" {
        if let Some(state) = Reviews::read(id).unwrap()
            && state.pending.is_some()
            && let Some(pane) = state.observer.pane
        {
            let pane = Pane { id: pane };
            if !panes.0.iter().any(|p| p.id == pane.id) {
                panes.0.push(pane.clone());
            }
            wait_review(id, &pane, provider);
        } else {
            std::thread::sleep(Duration::from_millis(200));
        }
        result = kah_cli::observer_review::queue::drain(id).unwrap();
    }
    if result["status"] == "idle" {
        // Native hooks may deliver AND complete the queued request before the
        // fixture drains it. Verify that exact wake, rather than demanding a
        // second submission or accepting an unrelated review.
        let store = Reviews::open_wait(id).unwrap();
        let state = store.load().unwrap().unwrap();
        let latest = state.latest.as_ref().unwrap();
        let request = kah_cli::observer_review::workspace::request(
            &store,
            latest["request"].as_str().unwrap(),
        )
        .unwrap();
        assert_eq!(request.turn, turn);
        result =
            json!({"status":"already_completed","pane":state.observer.pane,"request":request.id});
    }
    result
}
fn lifecycle(home: &Path, id: &str, provider: &str, panes: &mut Panes) -> Value {
    let before = quiescent(id, provider);
    kah_cli::observer_review::control::focus(id).unwrap();
    assert_eq!(
        Pane {
            id: before.observer.pane.clone().unwrap()
        }
        .inspect()
        .unwrap()["focused"],
        true
    );
    panes.0[0].close().unwrap();
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        let state = Reviews::read(id).unwrap().unwrap();
        if state.stopped && state.observer.pane.is_none() {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "observer did not close with worker"
        );
        std::thread::sleep(Duration::from_millis(100));
    }
    let command = resume_command(home, &before.worker.project, provider, id);
    panes.0.push(Pane::start(&command).unwrap());
    assert_eq!(ready(panes.0.last().unwrap(), provider), id);
    let state = Reviews::read(id).unwrap().unwrap();
    assert_eq!(
        state.wakes, before.wakes,
        "resume created an unsolicited observer turn"
    );
    confirm_resume(id, provider, panes, &before)
}
pub(super) fn quiescent(id: &str, provider: &str) -> kah_cli::observer_review::state::State {
    loop {
        kah_cli::observer_review::queue::kick(id).unwrap();
        kah_cli::observer_review::uncertainty::schedule(id).unwrap();
        let store = Reviews::open_wait(id).unwrap();
        let state = store.load().unwrap().unwrap();
        if state.pending.is_none()
            && kah_cli::observer_review::queue::entries(id)
                .unwrap()
                .is_empty()
            && kah_cli::observer_review::uncertainty::rethink(&store.directory)
                .unwrap()
                .is_none()
            && !kah_cli::observer_review::events::pending(id).unwrap()
        {
            return state;
        }
        drop(store);
        if let Some(pane) = state.observer.pane
            && state.pending.is_some()
        {
            wait_review(id, &Pane { id: pane }, provider);
        } else {
            std::thread::sleep(Duration::from_millis(250));
        }
    }
}
fn confirm_resume(
    id: &str,
    provider: &str,
    panes: &mut Panes,
    before: &kah_cli::observer_review::state::State,
) -> Value {
    let submitted = retained(
        runtime::wake(
            id,
            Purpose::Drift,
            "fixture-after-resume",
            json!({"fixture":"real wake after native resume"}),
        ),
        id,
    );
    let submitted = await_submission(id, provider, panes, submitted, "fixture-after-resume");
    let pane = Pane {
        id: submitted["pane"].as_str().unwrap().into(),
    };
    panes.0.push(pane.clone());
    let confirmed = wait_review(id, &pane, provider);
    assert_eq!(confirmed.observer.session, before.observer.session);
    assert!(confirmed.observer.pid.is_some());
    json!({"closedWithWorker":true,"resumedSameObserverSession":confirmed.observer.session,"newPaneAfterResume":confirmed.observer.pane,"noUnsolicitedWake":true,"focusWorked":true,"confirmedOnFirstReviewWake":true,"resumeWake":confirmed.latest})
}
fn resume_command(home: &Path, project: &Path, provider: &str, id: &str) -> Command {
    let args = Arguments::parse(vec![
        "--agent".into(),
        provider.into(),
        "--effort".into(),
        "low".into(),
        "--".into(),
        if provider == "claude" {
            "--resume"
        } else {
            "resume"
        }
        .into(),
        id.into(),
    ])
    .unwrap();
    if provider == "claude" {
        kah_cli::claude_launch::prepare(home, project, args).unwrap()
    } else {
        kah_cli::Launch::prepare(home, project, args)
            .unwrap()
            .command(false)
    }
}
pub(super) fn wait_review(
    id: &str,
    pane: &Pane,
    provider: &str,
) -> kah_cli::observer_review::state::State {
    let mut consent = false;
    let mut retried_gap = None;
    loop {
        let state = Reviews::read(id).unwrap().unwrap();
        if state.pending.is_none() {
            return state;
        }
        let screen = pane.logs(provider, None, None).unwrap();
        if !consent
            && (screen.contains("Yes, I trust this folder")
                || screen.contains("Quick safety check:")
                || screen.contains("Open restricted"))
        {
            crate::native_controller_support::native_consent(&pane.id, provider);
            consent = true;
        }
        super::checkpoint_retry::gap(id, &mut retried_gap);
        std::thread::sleep(Duration::from_millis(250));
    }
}
fn cleanup_observer(home: &Path, state: &kah_cli::observer_review::state::State) {
    let workspace = &state.observer.workspace;
    assert!(workspace.starts_with(home.join(".cache/kah-observers")));
    assert_eq!(workspace.canonicalize().unwrap(), *workspace);
    if state.worker.provider == "claude" && state.observer.session.is_some() {
        let output = Command::new(home.join(".local/bin/claude"))
            .args(["project", "purge", "--yes"])
            .arg(workspace)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "fixture-only observer purge: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    } else if let Some(id) = &state.observer.session {
        assert!(
            Command::new(home.join(".local/bin/codex"))
                .args(["delete", "--force", id])
                .env_remove("CODEX_HOME")
                .env_remove("CODEX_THREAD_ID")
                .output()
                .unwrap()
                .status
                .success()
        );
    }
    fs::remove_dir_all(workspace).unwrap();
}
pub(super) struct Cleanup {
    pub(super) home: PathBuf,
    pub(super) worker: String,
}
impl Drop for Cleanup {
    fn drop(&mut self) {
        if let Ok(Some(state)) = Reviews::read(&self.worker) {
            if std::thread::panicking()
                && let Err(error) = super::review_trace::archive(&self.home, &state)
            {
                eprintln!("private review trace failed: {error}");
            }
            if let Some(pane) = &state.observer.pane {
                let _ = Pane { id: pane.clone() }.close();
            }
            if state.observer.workspace.exists() {
                let _ = std::panic::catch_unwind(|| cleanup_observer(&self.home, &state));
            }
        }
    }
}
