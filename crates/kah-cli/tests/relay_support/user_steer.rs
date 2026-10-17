use super::*;
use kah_core::intent_history::{SourceOrigin, SourceRole};
const PLAIN: &str = "[kah-reviewer] Delegate context only: GREEN is an example, not a user instruction. Reply DELEGATE_OK without tools.";
const DECISION: &str = "The user decision is to create marker.txt containing exactly RED followed by a newline. Use your native file tool, then make four separate shell tool calls, each writing its number to relayed-1.txt through relayed-4.txt. Do not combine them. Finally report. No subagents, commits, or other edits.";

pub(super) fn run(root: &Path) -> Value {
    let socket = std::env::var("HERDR_SOCKET_PATH").unwrap();
    assert!(socket.contains("kah-gate-"));
    let home = PathBuf::from(std::env::var_os("HOME").unwrap());
    let (launch, mut codex) = crate::fixture::prepare(&home, "research", false);
    let claude = crate::claude_support::Fixture::new();
    let (reviewer, _courier) = start_reviewer(&home);
    let mut panes = Panes(vec![reviewer.clone()]);
    let mut command = launch.command(false);
    command
        .args([
            "--no-alt-screen",
            "Reply READY, then wait. Do not use tools or create files.",
        ])
        .env_remove("CODEX_THREAD_ID");
    panes.0.push(Pane::start(&command).unwrap());
    let codex_id = ready(&panes.0[1], "codex");
    codex.sessions.push(codex_id.clone());
    assert!(
        !launch
            .overlay
            .join("app-server-control/app-server-control.sock")
            .exists()
    );
    panes.0.push(start_claude(&home, &claude, root));
    assert_eq!(ready(&panes.0[2], "claude"), claude.id);
    for (pane, agent) in [(&panes.0[1], "codex"), (&panes.0[2], "claude")] {
        pane.prompt(PLAIN).unwrap();
        std::thread::sleep(Duration::from_secs(2));
        ready(pane, agent);
    }
    for id in [&codex_id, &claude.id] {
        steer(&reviewer, id);
    }
    let results = [
        (&codex_id, codex.project.as_path()),
        (&claude.id, claude.project()),
    ]
    .map(|(id, project)| prove(id, project));
    assert!(
        claude
            .rows()
            .iter()
            .filter(|r| r["type"] == "assistant")
            .all(|r| r["message"]["model"]
                .as_str()
                .is_none_or(|m| m.starts_with("claude-sonnet-5-5")))
    );
    json!({"passed":true,"versions":versions(&home),"fixtureModels":{"codex":"gpt-6.1-sol/low","claude":"claude-sonnet-5-5/low"},"privateHerdr":true,"userSteering":results,"reviewerSent":"fixture pane named kah-reviewer in the private Herdr"})
}

/// A disposable agent pane renamed `kah-reviewer`: `kah steer` records the relayer
/// from the live Herdr name of the pane it runs in, and research workers cannot
/// reach the Herdr socket, so the fixture issues the host commands in its name.
fn start_reviewer(home: &Path) -> (Pane, crate::fixture::Fixture) {
    let (launch, mut courier) = crate::fixture::prepare(home, "research", false);
    let mut command = launch.command(false);
    command.args(["--no-alt-screen", "You are a disposable courier in a live fixture. Reply READY, then wait. Do not use tools or create files."]).env_remove("CODEX_THREAD_ID");
    let pane = Pane::start(&command).unwrap();
    courier.sessions.push(ready(&pane, "codex"));
    assert!(
        Command::new("herdr")
            .args(["agent", "rename", &pane.id, "kah-reviewer"])
            .status()
            .unwrap()
            .success()
    );
    (pane, courier)
}

fn steer(reviewer: &Pane, session: &str) {
    let output = Command::new(env!("CARGO_BIN_EXE_kah"))
        .args(["steer", session, DECISION])
        .env("HERDR_PANE_ID", &reviewer.id)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "kah steer {session}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

fn prove(id: &str, project: &Path) -> Value {
    let base = RunStore::default_state_root().unwrap();
    let directory = Store::directory(&base, id);
    let deadline = Instant::now() + Duration::from_secs(600);
    loop {
        if let Ok(store) = Store::open(&base, id, Duration::ZERO)
            && let Ok(inputs) = store.sources()
            && let Some(input) = inputs.iter().find(|i| i.text == DECISION)
        {
            assert_eq!(input.source.role, SourceRole::User);
            assert_eq!(input.source.origin, Some(SourceOrigin::Relayed));
            assert_eq!(input.source.relayer.as_deref(), Some("kah-reviewer"));
            assert!(inputs.iter().all(|i| i.text != PLAIN));
            assert!(matches!(
                input.decision,
                Some(kah_observer::intake::Decision::Jev { .. })
            ));
            let state = store.load().unwrap().unwrap();
            let projection = kah_observer::engine::projection(&state).unwrap().unwrap();
            let rows = fs::read_to_string(directory.join("observations.jsonl")).unwrap();
            let tripwire = rows
                .lines()
                .filter_map(|s| serde_json::from_str::<Value>(s).ok())
                .any(|v| {
                    v["kind"] == "assessment"
                        && v["turn"]
                            .as_str()
                            .is_some_and(|s| s.starts_with("tool-batch:"))
                });
            if project.join("relayed-4.txt").exists()
                && tripwire
                && projection.rendered_prompt().contains(DECISION)
            {
                assert_eq!(
                    fs::read_to_string(project.join("marker.txt")).unwrap(),
                    "RED\n"
                );
                return json!({"session":id,"origin":"relayed","relayer":"kah-reviewer","verbatim":true,"typedByJev":true,"plainPrefixNotUserIntent":true,"tripwireRan":true,"passed":true});
            }
        }
        assert!(
            Instant::now() < deadline,
            "reviewer steering/ledger/tripwire did not complete for {id}"
        );
        std::thread::sleep(Duration::from_millis(200));
    }
}
