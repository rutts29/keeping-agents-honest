use super::{
    history,
    pty::{Terminal, quote},
};
use crate::fixture;
use kah_core::run_store::RunStore;
use kah_observer::{engine, session::Store};
use serde_json::{Value, json};
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

pub fn run() {
    let home = PathBuf::from(std::env::var_os("HOME").unwrap());
    let before = fs::read(home.join(".codex/config.toml")).unwrap();
    let results = [journey(&home, "codex"), journey(&home, "cx")];
    assert_eq!(before, fs::read(home.join(".codex/config.toml")).unwrap());
    let evidence = PathBuf::from(std::env::var_os("KAH_GATE_EVIDENCE").unwrap());
    fs::write(evidence.join("tui-routing-overrides.json"),serde_json::to_string_pretty(&json!({"passed":true,"realTui":true,"entrypoints":results,"globalConfigUnchanged":true})).unwrap()).unwrap();
}
fn journey(home: &Path, entry: &str) -> Value {
    let (_, mut fixture) = fixture::prepare(home, "research", false);
    let mut reader = history::connect(home, &fixture.project);
    let command = format!(
        "bash --noprofile -ic {}",
        quote(&format!(
            "cd {}; {entry} --model gpt-6-sol --no-alt-screen {}",
            quote(fixture.project.to_str().unwrap()),
            quote("aside: Initialize this disposable session; reply READY without tools.")
        ))
    );
    let mut terminal = Terminal::start(&command, &fixture.stderr);
    terminal.wait_for("Open restricted");
    terminal.send("\r");
    let id = history::session(&mut reader, &fixture.project, &mut terminal);
    fixture.sessions.push(id.clone());
    let initial = history::completed_turn(&mut reader, &id, None);
    let steer = "  StEeR: Use native exec_command once to run: test ! -e /home/example/.codex/auth.json && echo KAH_ROUTE_GVISOR. Then native apply_patch creates forced.txt containing TUI_FORCED. No other tools, dependencies, subagents or commits. Report and stop.";
    terminal.submit(steer);
    let forced = history::completed_turn(&mut reader, &id, initial["id"].as_str());
    assert_eq!(
        fs::read_to_string(fixture.project.join("forced.txt"))
            .unwrap()
            .trim(),
        "TUI_FORCED"
    );
    assert!(forced["items"].as_array().unwrap().iter().any(|i| {
        i["type"] == "commandExecution"
            && i["aggregatedOutput"]
                .as_str()
                .is_some_and(|s| s.contains("KAH_ROUTE_GVISOR"))
    }));
    terminal.submit(
        "  AsIdE: Consider creating notes.txt with NOTES. Do not create it; reply ASIDE_SEEN only.",
    );
    history::completed_turn(&mut reader, &id, forced["id"].as_str());
    assert!(!fixture.project.join("notes.txt").exists());
    let state = Store::read(&RunStore::default_state_root().unwrap(), &id)
        .unwrap()
        .unwrap();
    assert!(state.gap.is_none(), "{:?}", state.gap);
    let projection = engine::projection(&state).unwrap().unwrap();
    assert_eq!(projection.frontier.len(), 1);
    assert!(projection.frontier[0].text.starts_with("StEeR:"));
    assert!(!projection.frontier[0].review_required);
    let log = fs::read_to_string(
        Store::directory(&RunStore::default_state_root().unwrap(), &id).join("observations.jsonl"),
    )
    .unwrap();
    assert!(
        !log.lines()
            .filter_map(|l| serde_json::from_str::<Value>(l).ok())
            .any(|v| v["purpose"] == "steering")
    );
    terminal.close();
    assert!(terminal.output().contains("kah: research project"));
    check_bypass(&fixture.project);
    json!({"entrypoint":entry,"realBashrcRouting":true,"gvisorHostAuthHidden":true,"observerAttached":true,"typedSteerRecorded":true,"typedAsideExcluded":true,"steeringJudgeCalls":0,"commandCodexBypasses":true})
}

fn check_bypass(project: &Path) {
    let bypass = Command::new("bash")
        .args(["--noprofile", "-ic", "command codex --version"])
        .current_dir(project)
        .output()
        .unwrap();
    assert!(bypass.status.success());
    assert!(!String::from_utf8_lossy(&bypass.stderr).contains("kah:"));
}
