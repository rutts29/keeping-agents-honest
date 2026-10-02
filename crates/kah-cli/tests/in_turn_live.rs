#[allow(dead_code)]
mod claude_support;
#[path = "journeys/fixture.rs"]
mod fixture;
#[allow(dead_code)]
#[path = "terminal_support/history.rs"]
mod history;
#[path = "terminal_support/hook_latency.rs"]
mod hook_latency;
#[allow(dead_code)]
#[path = "terminal_support/processes.rs"]
mod processes;
#[allow(dead_code)]
#[path = "terminal_support/pty.rs"]
mod pty;
#[path = "terminal_support/version.rs"]
mod version;
use kah_core::run_store::RunStore;
use kah_observer::session::Store;
use serde_json::{Value, json};
use std::{
    fs,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

fn task(file: &str, shell: &str) -> String {
    format!(
        "steer: Use {file} to create marker.txt containing BLUE. Keep it BLUE throughout. Then make six separate {shell} tool calls, each writing its number to tick-1.txt through tick-6.txt; do not batch them into one tool call. Finally use {shell} to run touch holding.txt; sleep 30. Wait for that command, then report. No other tools, subagents, commits or edits."
    )
}
#[test]
#[ignore = "live Claude checks occur during one long native turn, before Stop"]
fn claude_checks_inside_a_long_turn() {
    let version = version::Version::capture("claude");
    let f = claude_support::Fixture::new();
    let mut terminal = f.start(false);
    f.ready(&mut terminal);
    f.prompt(&mut terminal, &task("Write", "Bash"));
    f.wait(&mut terminal, || {
        f.project().join("holding.txt").exists() && checks(&f.base, &f.id) > 0
    });
    let rows = f.rows();
    assert!(
        !rows
            .iter()
            .rev()
            .find(|r| r["type"] == "assistant")
            .is_some_and(|r| r["message"]["stop_reason"] == "end_turn")
    );
    let measured = checks(&f.base, &f.id);
    assert!((1..=3).contains(&measured));
    assert_eq!(
        fs::read_to_string(f.project().join("marker.txt"))
            .unwrap()
            .trim(),
        "BLUE"
    );
    drop(terminal);
    if std::env::var_os("KAH_GATE_HOOK_LATENCY").is_some() {
        let home = PathBuf::from(std::env::var_os("HOME").unwrap());
        let transcript = home
            .join(".claude/projects")
            .join(f.project().to_string_lossy().replace('/', "-"))
            .join(format!("{}.jsonl", f.id));
        save(
            "claude-latency",
            hook_latency::measure(&f.base, &f.id, true, &transcript),
        );
    }
    let installed = installed("claude");
    version.verify(&json!(installed));
    save(
        "claude",
        json!({"version":installed,"fixtureModel":"claude-sonnet-5-5","fixtureEffort":"low","inTurnChecks":measured,"beforeEndTurn":true,"markerPreserved":true,"tripwireUsage":Store::read(&f.base,&f.id).unwrap().unwrap().usage,"passed":true}),
    );
}
#[test]
#[ignore = "live Codex checks occur during one long native turn, before Stop"]
fn codex_checks_inside_a_long_turn() {
    let version = version::Version::capture("codex");
    let home = PathBuf::from(std::env::var_os("HOME").unwrap());
    let (launch, mut f) = fixture::prepare(&home, "research", false);
    let mut command = launch.command(false);
    command.args(["--no-alt-screen", &task("apply_patch", "exec_command")]);
    let mut terminal = pty::Terminal::start(&pty::command_line(&command), &f.stderr);
    terminal.wait_for("Open restricted");
    terminal.send("\r");
    let mut client = history::connect(&home, &f.project);
    let id = history::session(&mut client, &f.project, &mut terminal);
    f.sessions.push(id.clone());
    let base = RunStore::default_state_root().unwrap();
    let deadline = Instant::now() + Duration::from_secs(180);
    while !f.project.join("holding.txt").exists() || checks(&base, &id) == 0 {
        terminal.respond_to_cursor_queries();
        assert!(
            Instant::now() < deadline,
            "no in-turn observation before fixture deadline"
        );
        std::thread::sleep(Duration::from_millis(100));
    }
    let turns = client
        .request(
            "thread/turns/list",
            json!({"threadId":id,"limit":1,"sortDirection":"desc","itemsView":"notLoaded"}),
        )
        .unwrap();
    let read = client
        .request("thread/read", json!({"threadId":id,"includeTurns":false}))
        .unwrap();
    assert_open_rollout(
        Path::new(read["thread"]["path"].as_str().unwrap()),
        turns["data"][0]["id"].as_str().unwrap(),
    );
    let measured = checks(&base, &id);
    assert!((1..=3).contains(&measured));
    assert_eq!(
        fs::read_to_string(f.project.join("marker.txt"))
            .unwrap()
            .trim(),
        "BLUE"
    );
    drop(terminal);
    if std::env::var_os("KAH_GATE_HOOK_LATENCY").is_some() {
        save(
            "codex-latency",
            hook_latency::measure(
                &base,
                &id,
                false,
                Path::new(read["thread"]["path"].as_str().unwrap()),
            ),
        );
    }
    let installed = installed("codex");
    version.verify(&json!(installed));
    save(
        "codex",
        json!({"version":installed,"fixtureModel":"gpt-6.1-sol","fixtureEffort":"low","inTurnChecks":measured,"beforeTurnCompleted":true,"sidecarTurnStatus":turns["data"][0]["status"],"nativeRolloutStillOpen":true,"markerPreserved":true,"tripwireUsage":Store::read(&base,&id).unwrap().unwrap().usage,"passed":true}),
    );
}
fn assert_open_rollout(path: &Path, turn: &str) {
    let events = fs::read_to_string(path)
        .unwrap()
        .lines()
        .filter_map(|line| serde_json::from_str::<Value>(line).ok())
        .filter(|row| row["type"] == "event_msg" && row["payload"]["turn_id"] == turn)
        .map(|row| row["payload"]["type"].clone())
        .collect::<Vec<_>>();
    assert!(events.contains(&json!("task_started")), "{events:?}");
    assert!(
        !events.contains(&json!("task_complete")) && !events.contains(&json!("turn_aborted")),
        "{events:?}"
    );
}
fn checks(base: &Path, id: &str) -> usize {
    fs::read_to_string(Store::directory(base, id).join("observations.jsonl"))
        .unwrap_or_default()
        .lines()
        .filter_map(|line| serde_json::from_str::<Value>(line).ok())
        .filter(|row| {
            row["kind"] == "assessment"
                && row["turn"]
                    .as_str()
                    .is_some_and(|s| s.starts_with("tool-batch:"))
        })
        .count()
}
fn installed(agent: &str) -> String {
    let binary = PathBuf::from(std::env::var_os("HOME").unwrap())
        .join(".local/bin")
        .join(agent);
    String::from_utf8(
        std::process::Command::new(binary)
            .arg("--version")
            .output()
            .unwrap()
            .stdout,
    )
    .unwrap()
    .trim()
    .into()
}
fn save(agent: &str, value: Value) {
    let root = PathBuf::from(std::env::var_os("KAH_GATE_EVIDENCE").unwrap());
    fs::write(
        root.join(format!("in-turn-{agent}.json")),
        serde_json::to_vec_pretty(&value).unwrap(),
    )
    .unwrap();
}
