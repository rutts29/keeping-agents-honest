#[path = "journeys/fixture.rs"]
mod fixture;
#[allow(dead_code)]
#[path = "terminal_support/history.rs"]
mod history;
#[allow(dead_code)]
#[path = "terminal_support/processes.rs"]
mod processes;
#[allow(dead_code)]
#[path = "terminal_support/pty.rs"]
mod pty;
#[path = "terminal_support/public_config.rs"]
mod public_config;
use kah_cli::{Arguments, Launch};
use kah_codex::overlay;
use kah_sandbox::{Executor, LaunchOptions, current_uid, publish_binary};
use pty::Terminal;
use serde_json::{Value, json};
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

#[test]
#[ignore = "native TUI: hosted search and online MCP with offline project commands"]
fn network_search_and_mcp_do_not_require_project_egress() {
    let home = PathBuf::from(std::env::var_os("HOME").unwrap());
    let before = globals(&home);
    let (launch, mut f) = fixture::prepare(&home, "research", false);
    online_mcp(&home, &launch, &f);
    let prompt = "Use native exec_command once: curl --max-time 8 -Is https://example.com; echo SHELL_CURL_EXIT=$?. Then invoke fixture MCP probe (native tool discovery is allowed). Finally use hosted web search to find IANA's documentation about example domains. Perform the actual search. No other tools, subagents, commits or installs; summarize and stop.";
    let mut command = launch.command(false);
    command.args(["--no-alt-screen", prompt]);
    let mut terminal = Terminal::start(&pty::command_line(&command), &f.stderr);
    terminal.wait_for("Open restricted");
    terminal.send("\r");
    let mut reader = history::connect(&home, &f.project);
    let id = history::session(&mut reader, &f.project, &mut terminal);
    f.sessions.push(id.clone());
    let turn = history::completed_turn(&mut reader, &id, None);
    assert!(outputs(&turn).contains("SHELL_CURL_EXIT=6"));
    assert!(
        turn["items"]
            .as_array()
            .unwrap()
            .iter()
            .any(|v| v["type"] == "webSearch"),
        "native hosted search absent"
    );
    let proof: Value =
        serde_json::from_slice(&fs::read(f.project.join("mcp-proof.json")).unwrap()).unwrap();
    assert_eq!(proof["networkExit"], 0);
    assert_eq!(proof["hostCanaryHidden"], true);
    assert_eq!(proof["hostLoginHidden"], true);
    drop(terminal);
    drop(reader);
    drop(f);
    assert_eq!(before, globals(&home));
    save(
        "split-egress.json",
        json!({"version":version(),"fixtureModel":"gpt-6.1-sol","fixtureEffort":"low","domain":"research-offline","nativeHostedSearch":true,"shellNetworkBlocked":true,"onlineMcp":proof,"globalConfigUnchanged":true,"passed":true}),
    );
}

#[test]
#[ignore = "native Untrusted --online TUI and native resume without model overrides"]
fn network_untrusted_online_survives_resume() {
    let home = PathBuf::from(std::env::var_os("HOME").unwrap());
    let before = globals(&home);
    let (launch, mut f) = fixture::prepare(&home, "untrusted", true);
    let task = "Use native exec_command once: curl --max-time 10 -Is https://example.com; echo ONLINE_CURL_EXIT=$?. No other tools or commits; report and stop.";
    let mut command = launch.command(false);
    command.args(["--no-alt-screen", task]);
    let mut terminal = Terminal::start(&pty::command_line(&command), &f.stderr);
    terminal.wait_for("Open restricted");
    terminal.send("\r");
    let mut reader = history::connect(&home, &f.project);
    let id = history::session(&mut reader, &f.project, &mut terminal);
    f.sessions.push(id.clone());
    let first = history::completed_turn(&mut reader, &id, None);
    assert!(outputs(&first).contains("ONLINE_CURL_EXIT=0"));
    drop(terminal);
    let args = Arguments::parse(vec![
        "--".into(),
        "resume".into(),
        id.clone(),
        "--no-alt-screen".into(),
        task.into(),
    ])
    .unwrap();
    let resumed = Launch::prepare(&home, &home.join("repos/mine/kah"), args).unwrap();
    assert_eq!(resumed.project.root, f.project);
    assert_eq!(
        resumed.environment_id.as_deref(),
        Some("kah-untrusted-online")
    );
    let mut terminal = Terminal::start(&pty::command_line(&resumed.command(false)), &f.stderr);
    terminal.wait_for("Open restricted");
    let second = history::resumed_turn(
        &mut reader,
        &id,
        first["id"].as_str().unwrap(),
        &mut terminal,
    );
    assert!(outputs(&second).contains("ONLINE_CURL_EXIT=0"));
    drop(terminal);
    drop(reader);
    drop(f);
    assert_eq!(before, globals(&home));
    save(
        "untrusted-online.json",
        json!({"version":version(),"fixtureInitialModel":"gpt-6.1-sol","fixtureInitialEffort":"low","domain":"untrusted-online","egressPassed":true,"resumeEgressPassed":true,"resumeWithoutChoiceOverrides":true,"globalConfigUnchanged":true,"passed":true}),
    );
}

fn online_mcp(home: &Path, launch: &Launch, f: &fixture::Fixture) {
    fs::write(
        f.project.join("fixture-server.js"),
        include_str!("native_extensions_support/server.js"),
    )
    .unwrap();
    let binary = publish_binary(
        &home.join(".local/bin/codex"),
        &PathBuf::from(format!("/var/tmp/kah-runtime-{}", current_uid().unwrap())),
    )
    .unwrap();
    let image = fs::read_to_string("/usr/local/share/sbx/image-ref").unwrap();
    let uid = kah_run::access::service_uid().unwrap();
    let offline = Executor::new(
        &launch.project,
        LaunchOptions::default(),
        &binary,
        image.trim(),
        uid,
    )
    .unwrap();
    let online = Executor::new(
        &launch.project,
        LaunchOptions {
            online: true,
            yolo: false,
        },
        &binary,
        image.trim(),
        uid,
    )
    .unwrap();
    let id = launch.environment_id.as_deref().unwrap();
    overlay::set_executors(
        &f.overlay,
        id,
        &[(id, &offline.args), ("mcp-online", &online.args)],
    )
    .unwrap();
    overlay::write_settings(&f.overlay,json!({"mcp_servers":{"fixture":{"command":"/usr/bin/node","args":[f.project.join("fixture-server.js"),f.canary],"cwd":f.project,"environment_id":"mcp-online"}}})).unwrap();
}
fn outputs(turn: &Value) -> String {
    turn["items"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|v| v["type"] == "commandExecution")
        .map(|v| v["aggregatedOutput"].as_str().unwrap_or(""))
        .collect()
}
// Same optional native hook source as native_extensions_live; absence is audited.
fn globals(home: &Path) -> (Vec<u8>, Option<Vec<u8>>) {
    (
        fs::read(home.join(".codex/config.toml")).unwrap(),
        public_config::snapshot(&home.join(".codex/hooks.json")),
    )
}
fn version() -> String {
    String::from_utf8(
        Command::new("/home/example/.local/bin/codex")
            .arg("--version")
            .output()
            .unwrap()
            .stdout,
    )
    .unwrap()
    .trim()
    .into()
}
fn save(name: &str, evidence: Value) {
    let root = PathBuf::from(std::env::var_os("KAH_GATE_EVIDENCE").unwrap());
    fs::write(
        root.join(name),
        serde_json::to_vec_pretty(&evidence).unwrap(),
    )
    .unwrap();
}
