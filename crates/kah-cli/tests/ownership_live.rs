mod claude_restricted_support;
#[allow(dead_code)]
mod claude_support;
#[path = "journeys/fixture.rs"]
mod fixture;
#[allow(dead_code)]
#[path = "terminal_support/history.rs"]
mod history;
mod ownership_support;
#[allow(dead_code)]
#[path = "terminal_support/processes.rs"]
mod processes;
#[allow(dead_code)]
#[path = "terminal_support/pty.rs"]
mod pty;
use ownership_support::{BASH_CREATE, BASH_FINISH, prepare, verify_host};
use pty::Terminal;
use serde_json::json;
use std::{fs, path::PathBuf, process::Command};

#[test]
#[ignore = "native Claude Edit/Write interleaved with gVisor Bash and plain-host Git"]
fn ownership_claude_native_and_gvisor() {
    let f = claude_restricted_support::Restricted::new();
    f.verify_cwd_file();
    prepare(f.fixture.project());
    let before = claude_support::settings::snapshot(&f.home);
    let mut terminal = f.start();
    f.fixture.ready(&mut terminal);
    let task = format!(
        "steer: Authorized disposable ownership test. Perform these steps in order. First use Bash to run: {BASH_CREATE}. Then native Read and Edit must change ownership/from-bash.txt from ALPHA to BETA; native Write must create ownership/from-native.txt with NATIVE followed by newline. Then use Bash to run: {BASH_FINISH}. Do not substitute shell edits for native file tools. No other tools or subagents, installs, remote actions or permission workarounds; report and stop."
    );
    f.fixture.prompt(&mut terminal, &task);
    f.fixture.wait(&mut terminal, || {
        f.fixture
            .has_file("ownership/from-native.txt", "NATIVE\nBASH_APPEND")
    });
    f.fixture.settled(&mut terminal, &task);
    drop(terminal);
    let proof = verify_host(f.fixture.project());
    assert_eq!(before, claude_support::settings::snapshot(&f.home));
    save(
        "claude-ownership.json",
        json!({"version":version("claude"),"domain":"research-offline","fixtureModel":"claude-sonnet-5-5","fixtureEffort":"low","proof":proof,"globalConfigUnchanged":true,"passed":true}),
    );
}

#[test]
#[ignore = "native Codex apply_patch interleaved with gVisor commands and plain-host Git"]
fn ownership_codex_native_and_gvisor() {
    let home = PathBuf::from(std::env::var_os("HOME").unwrap());
    let before = fs::read(home.join(".codex/config.toml")).unwrap();
    let (launch, mut f) = fixture::prepare(&home, "research", false);
    prepare(&f.project);
    let task = format!(
        "Authorized disposable ownership test. In order: native exec_command runs `{BASH_CREATE}`; then native apply_patch changes ownership/from-bash.txt from ALPHA to BETA and adds ownership/from-native.txt with NATIVE followed by newline; then native exec_command runs `{BASH_FINISH}`. Do not substitute shell edits for apply_patch. No other tools, subagents, installs, remote actions or permission workarounds; report and stop."
    );
    let mut command = launch.command(false);
    command.args(["--no-alt-screen", &task]);
    let mut terminal = Terminal::start(&pty::command_line(&command), &f.stderr);
    terminal.wait_for("Open restricted");
    terminal.send("\r");
    let mut reader = history::connect(&home, &f.project);
    let id = history::session(&mut reader, &f.project, &mut terminal);
    f.sessions.push(id.clone());
    let turn = history::completed_turn(&mut reader, &id, None);
    assert!(
        turn["items"]
            .as_array()
            .unwrap()
            .iter()
            .any(|v| v["type"] == "fileChange" && v["status"] == "completed")
    );
    drop(terminal);
    drop(reader);
    let proof = verify_host(&f.project);
    drop(f);
    assert_eq!(before, fs::read(home.join(".codex/config.toml")).unwrap());
    save(
        "codex-ownership.json",
        json!({"version":version("codex"),"domain":"research-offline","fixtureModel":"gpt-6.1-sol","fixtureEffort":"low","proof":proof,"globalConfigUnchanged":true,"passed":true}),
    );
}

fn version(agent: &str) -> String {
    let binary = PathBuf::from(std::env::var_os("HOME").unwrap())
        .join(".local/bin")
        .join(agent);
    String::from_utf8(
        Command::new(binary)
            .arg("--version")
            .output()
            .unwrap()
            .stdout,
    )
    .unwrap()
    .trim()
    .into()
}
fn save(name: &str, value: serde_json::Value) {
    let root = PathBuf::from(std::env::var_os("KAH_GATE_EVIDENCE").unwrap());
    fs::write(root.join(name), serde_json::to_vec_pretty(&value).unwrap()).unwrap();
}
