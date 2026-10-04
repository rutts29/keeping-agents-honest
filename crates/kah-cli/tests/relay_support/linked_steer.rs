use super::*;
use kah_core::intent_history::IntentLifecycle;
const START: &str = "Reply READY, then await further user directions.";
const OLD: &str = "steer: Files named marker.txt must contain exactly BLUE followed by one newline. Reply OLD_SEEN; do not create or edit files yet.";
const NEW: &str = "steer: Files named marker.txt must contain exactly RED followed by one newline. This replaces the receipt-linked prior rule. Write marker.txt with RED and reply NEW_SEEN.";

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
    let outcomes = ["codex", "claude"].map(|provider| provider_case(&home, root, provider));
    for (path, bytes) in configs {
        assert_eq!(fs::read(path).unwrap(), bytes);
    }
    json!({"passed":true,"versions":versions(&home),"floorModels":{"codex":"gpt-6.1-sol/low","claude":"claude-sonnet-5-5/low"},"privateHerdr":true,"nativeConfigsUnchanged":true,"linkedSteering":outcomes})
}
fn provider_case(home: &Path, root: &Path, provider: &str) -> Value {
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
    let panes = Panes(vec![pane]);
    let id = ready(&panes.0[0], provider);
    if provider == "codex" {
        codex.sessions.push(id.clone());
    }
    let cleanup = super::review::Cleanup {
        home: home.to_path_buf(),
        worker: id.clone(),
    };
    let project = claude
        .as_ref()
        .map(|f| f.project())
        .unwrap_or(&codex.project);
    let old = format!("linked-old-{}", kah_observer::session::now_ms());
    let new = format!("linked-new-{}", kah_observer::session::now_ms());
    steer(home, &id, OLD, &old, None);
    wait_input(&id, OLD);
    ready(&panes.0[0], provider);
    steer(home, &id, NEW, &new, Some(&old));
    wait_input(&id, NEW);
    ready(&panes.0[0], provider);
    let outcome = prove(&id, project, provider, &old, &new);
    drop(panes);
    drop(cleanup);
    outcome
}
fn prove(id: &str, project: &Path, provider: &str, old: &str, new: &str) -> Value {
    let primary = Store::open(
        &RunStore::default_state_root().unwrap(),
        id,
        Duration::from_secs(30),
    )
    .unwrap();
    let sources = primary.sources().unwrap();
    let older = sources.iter().find(|i| i.text == OLD).unwrap();
    let newer = sources.iter().find(|i| i.text == NEW).unwrap();
    assert_eq!(newer.source.supersedes_receipt.as_deref(), Some(old));
    let state = primary.load().unwrap().unwrap();
    let history = RunStore::open_default(project, state.run_id.as_deref().unwrap())
        .unwrap()
        .recover()
        .unwrap()
        .history;
    assert_eq!(
        history.intent(older.id()).unwrap().lifecycle,
        IntentLifecycle::Withdrawn
    );
    assert_eq!(history.intent(older.id()).unwrap().text, OLD);
    assert!(
        history
            .intent(older.id())
            .unwrap()
            .changed_by
            .contains(&newer.source)
    );
    assert!(history.intent(newer.id()).unwrap().is_active());
    assert_eq!(
        fs::read_to_string(project.join("marker.txt")).unwrap(),
        "RED\n"
    );
    json!({"provider":provider,"worker":id,"olderReceipt":old,"newerReceipt":new,"linkPreserved":true,"olderWithdrawn":true,"verbatim":true,"redMarker":true})
}
pub(super) fn steer(home: &Path, id: &str, text: &str, receipt: &str, supersedes: Option<&str>) {
    let mut command = Command::new(home.join(".local/bin/kah"));
    command.args(["steer", id, text, "--receipt", receipt]);
    if let Some(previous) = supersedes {
        command.args(["--supersedes", previous]);
    }
    let output = command.output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}
pub(super) fn wait_input(id: &str, text: &str) {
    let deadline = Instant::now() + Duration::from_secs(600);
    loop {
        if let Ok(primary) =
            Store::open(&RunStore::default_state_root().unwrap(), id, Duration::ZERO)
            && primary.sources().unwrap().iter().any(|i| i.text == text)
        {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "native relayed source never reached the ledger"
        );
        std::thread::sleep(Duration::from_millis(200));
    }
}
