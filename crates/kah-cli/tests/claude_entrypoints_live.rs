#[allow(dead_code)]
mod claude_support;
#[path = "terminal_support/herdr_probe.rs"]
mod herdr_probe;
#[allow(dead_code)]
#[path = "terminal_support/processes.rs"]
mod processes;
#[allow(dead_code)]
#[path = "terminal_support/pty.rs"]
mod pty;
use claude_support::Fixture;
use kah_observer::session::Store;
use pty::{Terminal, quote};
use serde_json::json;
use std::{fs, process::Command};

#[test]
fn herdr_capture_entrypoint() {
    if std::env::var_os("KAH_HERDR_CAPTURE").is_some() {
        herdr_probe::capture();
    }
}

#[test]
#[ignore = "real isolated Herdr session, newly created Mine project and native Claude task"]
fn claude_herdr_and_new_project() {
    let f = Fixture::new();
    f.reserve_for_creation();
    let probe = herdr_probe::setup();
    let version = Command::new("claude").arg("--version").output().unwrap();
    let settings =
        std::path::PathBuf::from(std::env::var_os("HOME").unwrap()).join(".claude/settings.json");
    let before = fs::read(&settings).unwrap();
    let mut terminal = Terminal::start(
        &format!(
            "HERDR_CONFIG_PATH={} herdr --session {}",
            quote(probe.root.join("config.toml").to_str().unwrap()),
            quote(&probe.name)
        ),
        &probe.root.join("terminal.log"),
    );
    terminal.wait_for("KAH_HERDR_SHELL_READY");
    terminal.submit(&launch(&f, &probe));
    f.ready(&mut terminal);
    let task = "steer: Use native Write to create herdr-proof.txt containing HERDR_WORKS. No other tools, files, subagents or commits. Report and stop.";
    f.prompt(&mut terminal, task);
    f.wait(&mut terminal, || {
        f.has_file("herdr-proof.txt", "HERDR_WORKS")
    });
    f.settled(&mut terminal, task);
    let capture = herdr_probe::wait_capture(&probe);
    assert_eq!(capture["pane"]["agent"], "claude");
    assert_eq!(capture["pane"]["agent_session"]["value"], f.id);
    assert!(
        capture["states"]
            .as_array()
            .unwrap()
            .contains(&json!("working"))
    );
    let commit = Command::new("git")
        .arg("-C")
        .arg(f.project())
        .args(["rev-parse", "--verify", "HEAD"])
        .output()
        .unwrap();
    assert!(commit.status.success());
    assert!(f.project().join("AGENTS.md").is_file());
    assert!(f.project().join("CLAUDE.md").is_file());
    assert!(Store::read(&f.base, &f.id).unwrap().unwrap().gap.is_none());
    let offset = terminal.log_size();
    terminal.submit("/exit");
    terminal.wait_for_since("KAH_HERDR_SHELL_READY", offset);
    terminal.submit(&format!("herdr session stop {} --json", quote(&probe.name)));
    terminal.wait_exit(false);
    drop(terminal);
    assert!(
        before == fs::read(&settings).unwrap(),
        "global Claude settings changed"
    );
    fs::write(f.evidence.join("herdr-new-project.json"), serde_json::to_vec_pretty(&json!({"version":String::from_utf8(version.stdout).unwrap().trim(),
        "passed":true,"fixtureModel":"claude-sonnet-5-5","agent":capture["pane"]["agent"],"sessionIdentityMatched":true,"states":capture["states"],
        "newProjectFirstCommit":String::from_utf8(commit.stdout).unwrap().trim(),"guidanceFilesPresent":true,"nativeWrite":true,"observerGap":false,
        "globalSettingsUnchanged":true})).unwrap()).unwrap();
}

fn launch(f: &Fixture, probe: &herdr_probe::Probe) -> String {
    format!(
        "test \"$HERDR_ENV\" = 1 && (KAH_HERDR_CAPTURE={} KAH_HERDR_MARKER={} {} --exact herdr_capture_entrypoint --nocapture > {} 2>&1 &) && {} --agent claude mine {} --model claude-sonnet-5-5 --effort low -- --session-id {} --permission-mode acceptEdits",
        quote(probe.root.join("capture.json").to_str().unwrap()),
        quote(f.project().join("herdr-proof.txt").to_str().unwrap()),
        quote(std::env::current_exe().unwrap().to_str().unwrap()),
        quote(probe.root.join("helper.log").to_str().unwrap()),
        quote(env!("CARGO_BIN_EXE_kah")),
        quote(f.project().file_name().unwrap().to_str().unwrap()),
        quote(&f.id)
    )
}
