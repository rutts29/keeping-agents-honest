//! A real tentative replacement must be verified before ledger application.
use super::*;
use kah_cli::observer_review::state::Store as Reviews;
use kah_observer::intake::Decision;
const START: &str = "Reply READY and await further user instructions.";
const OLD: &str = "steer: Files named marker.txt must contain exactly BLUE followed by one newline. Reply OLD_SEEN; do not edit files yet.";
const ASK: &str = "Could the marker requirement be RED instead? This might replace our BLUE constraint; I have not decided yet. Do not change any files; explain the implications.";

pub fn run(root: &Path) -> Value {
    assert!(
        std::env::var("HERDR_SOCKET_PATH")
            .unwrap()
            .contains("kah-gate-")
    );
    let home = PathBuf::from(std::env::var_os("HOME").unwrap());
    let before = [
        home.join(".codex/config.toml"),
        home.join(".claude/settings.json"),
    ]
    .map(|p| (p.clone(), fs::read(p).unwrap()));
    let outcomes = ["codex", "claude"].map(|provider| case(&home, root, provider));
    for (path, bytes) in before {
        assert_eq!(fs::read(path).unwrap(), bytes)
    }
    json!({"versions":versions(&home),"models":{"codex":"gpt-6.1-sol/low","claude":"claude-sonnet-5-5/low"},"uncertainSteering":outcomes,"nativeConfigsUnchanged":true,"privateHerdr":true,"passed":true})
}
fn case(home: &Path, root: &Path, provider: &str) -> Value {
    let (launch, mut codex) = crate::fixture::prepare(home, "research", false);
    let claude =
        (provider == "claude").then(|| crate::claude_support::Fixture::new_domain("research"));
    let pane = if provider == "codex" {
        let mut command = launch.command(false);
        command
            .args(["--no-alt-screen", START])
            .env_remove("CODEX_THREAD_ID");
        Pane::start(&command).unwrap()
    } else {
        start_claude_prompt(home, claude.as_ref().unwrap(), root, START)
    };
    let mut panes = Panes(vec![pane]);
    let id = ready(&panes.0[0], provider);
    if provider == "codex" {
        codex.sessions.push(id.clone())
    }
    let cleanup = super::review::Cleanup {
        home: home.into(),
        worker: id.clone(),
    };
    let project = claude
        .as_ref()
        .map(|f| f.project())
        .unwrap_or(&codex.project);
    linked_steer::steer(
        home,
        &id,
        OLD,
        &format!("uncertain-old-{}", kah_observer::session::now_ms()),
        None,
    );
    linked_steer::wait_input(&id, OLD);
    ready(&panes.0[0], provider);
    linked_steer::steer(
        home,
        &id,
        ASK,
        &format!("uncertain-ask-{}", kah_observer::session::now_ms()),
        None,
    );
    linked_steer::wait_input(&id, ASK);
    let initial = initial(&id);
    let review = reviewed(&id, provider, &mut panes);
    let verbatim = prove(&id, project);
    let result = json!({"provider":provider,"worker":id,"input":ASK,"initialTripwire":initial,"fullReview":review,"blueConstraintStillActive":true,"noUnapprovedFileChange":true,"verbatim":verbatim});
    drop(panes);
    drop(cleanup);
    result
}
fn prove(id: &str, project: &Path) -> bool {
    let primary = Store::open(
        &RunStore::default_state_root().unwrap(),
        id,
        Duration::from_secs(2),
    )
    .unwrap();
    let sources = primary.sources().unwrap();
    let older = sources.iter().find(|i| i.text == OLD).unwrap();
    let newer = sources.iter().find(|i| i.text == ASK).unwrap();
    assert!(matches!(newer.decision, Some(Decision::Jev { .. })));
    let state = primary.load().unwrap().unwrap();
    let history = RunStore::open_default(project, state.run_id.as_deref().unwrap())
        .unwrap()
        .recover()
        .unwrap()
        .history;
    assert_eq!(
        history.intent(older.id()).unwrap().lifecycle,
        kah_core::intent_history::IntentLifecycle::Active
    );
    assert!(!project.join("marker.txt").exists());
    newer.text == ASK
}
fn initial(id: &str) -> Value {
    let primary = Store::open(
        &RunStore::default_state_root().unwrap(),
        id,
        Duration::from_secs(2),
    )
    .unwrap();
    let source = primary
        .sources()
        .unwrap()
        .into_iter()
        .find(|i| i.text == ASK)
        .unwrap();
    let Some(Decision::Verification { judgment }) = &source.decision else {
        panic!(
            "tentative input did not enter real high-impact verification: {}",
            serde_json::to_string(&source).unwrap()
        )
    };
    assert!(kah_observer::verification::uncertain_impact(judgment));
    let state = primary.load().unwrap().unwrap();
    let history = RunStore::open_default(&state.binding.project, state.run_id.as_deref().unwrap())
        .unwrap()
        .recover()
        .unwrap()
        .history;
    assert!(
        history.intent(source.id()).is_none(),
        "uncertain input applied before verification"
    );
    serde_json::to_value(judgment).unwrap()
}
fn reviewed(id: &str, provider: &str, panes: &mut Panes) -> Value {
    loop {
        if let Some(state) = Reviews::read(id).unwrap()
            && let Some(pane) = state.observer.pane
        {
            let pane = Pane { id: pane };
            if !panes.0.iter().any(|p| p.id == pane.id) {
                panes.0.push(pane.clone())
            }
            let state = super::review::wait_review(id, &pane, provider);
            if state
                .latest
                .as_ref()
                .is_some_and(|r| r["purpose"] == "steering")
            {
                return state.latest.unwrap();
            }
        }
        std::thread::sleep(Duration::from_millis(200));
    }
}
