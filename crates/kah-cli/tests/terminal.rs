#[path = "journeys/fixture.rs"]
mod fixture;
mod terminal_support;

use serde_json::{Value, json};
use std::{
    fs,
    path::{Path, PathBuf},
};
use terminal_support::{
    history,
    pty::{Terminal, quote},
};

#[test]
#[ignore = "isolated Herdr server with native Codex in a real PTY; gate only"]
fn herdr_terminal_entrypoint() {
    terminal_support::herdr::run();
}

// Subprocess entrypoint: the live test invokes this inside its own Herdr shell.
// During fast checks it has no environment trigger and performs no work.
#[test]
fn herdr_capture_entrypoint() {
    if std::env::var_os("KAH_HERDR_CAPTURE").is_some() {
        terminal_support::herdr::capture();
    }
}

#[test]
#[ignore = "native interactive TUI through a real PTY; gate only"]
fn terminal_entrypoints_preserve_project_policy() {
    let home = PathBuf::from(std::env::var_os("HOME").unwrap());
    let before = fs::read(home.join(".codex/config.toml")).unwrap();
    let (_, mut fixture) = fixture::prepare(&home, "research", false);
    let mut reader = history::connect(&home, &fixture.project);
    let prompt = "Use native exec_command once to run: pwd; test ! -e /home/example/.codex/auth.json && echo HOST_AUTH_HIDDEN; curl --max-time 8 -Is https://example.com; echo CURL_EXIT=$?. Then native apply_patch must create terminal-proof.txt containing TERMINAL_WORKS. Disposable launcher test: no other tools, subagents, installs or commits. Report and stop.";
    let command = shell(
        &fixture.project,
        &format!("codex --model gpt-6-sol --no-alt-screen {}", quote(prompt)),
    );
    let mut first = Terminal::start(&command, &fixture.stderr);
    first.wait_for("Open restricted");
    first.send("\r");
    let id = history::session(&mut reader, &fixture.project, &mut first);
    fixture.sessions.push(id.clone());
    let turn = history::completed_turn(&mut reader, &id, None);
    assert_eq!(
        fs::read_to_string(fixture.project.join("terminal-proof.txt"))
            .unwrap()
            .trim(),
        "TERMINAL_WORKS"
    );
    first.close();
    assert!(first.output().contains("kah: research project"));
    drop(first);
    let prompt = "Use native exec_command once to run: test -f terminal-proof.txt && echo PRIOR_EDIT_PRESENT; test ! -e /home/example/.codex/auth.json && echo HOST_AUTH_HIDDEN; curl --max-time 8 -Is https://example.com; echo CURL_EXIT=$?. Then native apply_patch must create terminal-resume-proof.txt containing RESUME_WORKS. No other tools, subagents or commits. Report and stop.";
    let command = shell(
        &home.join("repos/mine"),
        &format!("codex resume {id} --no-alt-screen {}", quote(prompt)),
    );
    let mut resumed = Terminal::start(&command, &fixture.stderr);
    resumed.wait_for("Open restricted");
    resumed.send("\r");
    let second = history::completed_turn(&mut reader, &id, turn["id"].as_str());
    assert_eq!(
        fs::read_to_string(fixture.project.join("terminal-resume-proof.txt"))
            .unwrap()
            .trim(),
        "RESUME_WORKS"
    );
    resumed.close();
    assert!(resumed.output().contains("kah: research project"));
    let operations = [checks(&turn), checks(&second)];
    drop(resumed);
    drop(reader);
    drop(fixture);
    assert_eq!(fs::read(home.join(".codex/config.toml")).unwrap(), before);
    let output = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../docs/gates/evidence/phase-2/terminal-automated.json");
    fs::write(output,serde_json::to_string_pretty(&json!({"turns":operations,"plainCodexRouted":true,"crossDirectoryResume":true,"globalConfigUnchanged":true,"passed":true})).unwrap()+"\n").unwrap();
}

fn shell(cwd: &Path, command: &str) -> String {
    let binary = Path::new(env!("CARGO_BIN_EXE_kah")).parent().unwrap();
    let routing = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/shell/kah.sh");
    let script = format!(
        "export PATH={}:\"$PATH\"; . {}; cd {}; {command}",
        quote(binary.to_str().unwrap()),
        quote(routing.to_str().unwrap()),
        quote(cwd.to_str().unwrap())
    );
    format!("bash --noprofile --norc -ic {}", quote(&script))
}

fn checks(turn: &Value) -> Value {
    let items: Vec<_> = turn["items"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|v| matches!(v["type"].as_str(), Some("commandExecution" | "fileChange")))
        .cloned()
        .collect();
    let outputs = items
        .iter()
        .filter(|v| v["type"] == "commandExecution")
        .map(|v| v["aggregatedOutput"].as_str().unwrap_or(""))
        .collect::<String>();
    assert!(outputs.contains("HOST_AUTH_HIDDEN"), "{items:?}");
    assert!(outputs.contains("CURL_EXIT=6"), "{items:?}");
    json!({"status":turn["status"],"operations":items})
}
