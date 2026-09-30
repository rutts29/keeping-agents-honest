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
    let second =
        history::resumed_turn(&mut reader, &id, turn["id"].as_str().unwrap(), &mut resumed);
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
    let output = evidence(
        "terminal-policy-resume.json",
        "phase-2/terminal-automated.json",
    );
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

#[path = "../../kah-codex/tests/live_support/client.rs"]
mod client;

#[test]
#[ignore = "native in-process /resume from Mine yolo to Research via a real PTY; gate only"]
fn in_process_resume_rejects_cross_domain_history() {
    use kah_cli::{Arguments, Launch};
    let home = PathBuf::from(std::env::var_os("HOME").unwrap());
    let before = fs::read(home.join(".codex/config.toml")).unwrap();
    let (_, mut research) = fixture::prepare(&home, "research", false);
    let source = Launch::prepare(
        &home,
        &research.project,
        Arguments::parse(vec![
            "--no-observer".into(),
            "--model".into(),
            "gpt-6-luna".into(),
            "--effort".into(),
            "low".into(),
        ])
        .unwrap(),
    )
    .unwrap();
    let mut client = client::Client::start_command(source.command(true), &research.stderr);
    let start = client.request("thread/start", json!({"cwd":research.project}));
    let id = start["thread"]["id"].as_str().unwrap();
    research.sessions.push(id.into());
    assert_eq!(
        client.turn(id, "Reply SOURCE_READY. No tools.")["status"],
        "completed"
    );
    drop(client);
    let (_, mut mine) = fixture::prepare(&home, "mine", false);
    let command = format!(
        "cd {} && {} --no-observer --yolo --model gpt-6-luna --effort low --no-alt-screen 'Reply MINE_READY. No tools.'",
        quote(mine.project.to_str().unwrap()),
        quote(env!("CARGO_BIN_EXE_kah"))
    );
    let mut terminal = Terminal::start(&command, &mine.stderr);
    terminal.wait_for("Folder access");
    terminal.send("\r");
    let mut reader = history::connect(&home, &mine.project);
    let mine_id = history::session(&mut reader, &mine.project, &mut terminal);
    mine.sessions.push(mine_id.clone());
    history::completed_turn(&mut reader, &mine_id, None);
    terminal.wait_for("Ask Codex to do anything");
    let resume_offset = terminal.log_size();
    terminal.submit(&format!("/resume {id}"));
    terminal.wait_for("SOURCE_READY");
    // Codex defers SessionStart hooks until the next turn (hook_runtime.rs).
    terminal.submit("Use native apply_patch to create unsafe-first.txt containing HOST_ESCAPE.");
    terminal.wait_for("reopen with kah research");
    let offset = terminal.log_size();
    terminal.submit("Use native apply_patch to create unsafe.txt containing HOST_ESCAPE.");
    terminal.wait_for_since("reopen with kah research", offset);
    assert_resume_boundary(&mine, &research, resume_offset);
    terminal.close();
    assert_eq!(before, fs::read(home.join(".codex/config.toml")).unwrap());
    fs::write(evidence("cross-domain-resume.json", "phase-3/terminal-resume.json"),serde_json::to_string_pretty(&json!({"nativeTuiSlashResume":true,"fromPolicy":"mine-yolo","savedPolicy":"research-offline","resumeRejectedBeforeTools":true,"subsequentUserPromptRejected":true,"hostEditAbsent":true,"globalConfigUnchanged":true,"passed":true})).unwrap()+"\n").unwrap();
}

fn assert_resume_boundary(
    mine: &fixture::Fixture,
    research: &fixture::Fixture,
    resume_offset: usize,
) {
    assert!(!research.project.join("unsafe.txt").exists());
    assert!(!mine.project.join("unsafe.txt").exists());
    assert!(!mine.project.join("unsafe-first.txt").exists());
    assert!(!research.project.join("unsafe-first.txt").exists());
    let log = fs::read(&mine.stderr).unwrap();
    let resumed_output = String::from_utf8_lossy(&log[resume_offset..]);
    assert!(!resumed_output.contains("Folder access"));
    assert!(!resumed_output.contains("invalid session start JSON"));
}

#[test]
#[ignore = "real installed bashrc/cx routing and typed steer:/aside: through the native TUI"]
fn closure_routing_and_plaintext_overrides() {
    terminal_support::closure::run();
}

#[test]
fn fixture_process_audit() {
    if std::env::var_os("KAH_POST_GATE_AUDIT").is_none() {
        return;
    }
    let processes = terminal_support::processes::leftovers();
    let report = processes
        .iter()
        .map(|p| json!({"pid":p.pid,"group":p.group,"cwd":p.cwd}))
        .collect::<Vec<_>>();
    let containers = terminal_support::processes::fixture_containers().unwrap();
    let report = json!({"fixtureProcesses":report,"fixtureContainers":containers,"passed":processes.is_empty()&&containers.is_empty()});
    println!("{report}");
    if let Some(directory) = std::env::var_os("KAH_GATE_EVIDENCE")
        .map(PathBuf::from)
        .filter(|p| p.is_dir())
    {
        fs::write(
            directory.join("process-audit.json"),
            serde_json::to_vec_pretty(&report).unwrap(),
        )
        .unwrap();
    }
    assert!(
        processes.is_empty() && containers.is_empty(),
        "live fixture processes or containers remain"
    );
}

#[test]
fn dropping_a_live_pty_reaps_its_process_group() {
    let base = std::env::temp_dir().join(format!("kah-pty-drop-{}", std::process::id()));
    fs::create_dir_all(&base).unwrap();
    let pidfile = base.join("pid");
    let command = format!(
        "sleep 300 & echo $! > {}; echo GROUP_READY; wait",
        quote(pidfile.to_str().unwrap())
    );
    let mut terminal = Terminal::start(&command, &base.join("log"));
    terminal.wait_for("GROUP_READY");
    let pid = fs::read_to_string(&pidfile)
        .unwrap()
        .trim()
        .parse::<u32>()
        .unwrap();
    drop(terminal);
    assert!(
        !terminal_support::processes::list()
            .iter()
            .any(|p| p.pid == pid && p.state != 'Z')
    );
    fs::remove_dir_all(base).unwrap();
}

#[test]
#[ignore = "real Mine/Research startup warnings, merged Herdr hooks and observer"]
fn clean_startup_in_each_domain() {
    terminal_support::herdr::startup_domains();
}

fn evidence(name: &str, fallback: &str) -> PathBuf {
    std::env::var_os("KAH_GATE_EVIDENCE")
        .map(PathBuf::from)
        .map(|p| p.join(name))
        .unwrap_or_else(|| {
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../docs/gates/evidence")
                .join(fallback)
        })
}
