#[allow(dead_code)]
mod claude_support;
#[allow(dead_code)]
#[path = "terminal_support/processes.rs"]
mod processes;
#[allow(dead_code)]
#[path = "terminal_support/pty.rs"]
mod pty;

use claude_support::Fixture;
use kah_observer::{engine, session::Store};
use serde_json::{Value, json};
use std::{fs, process::Command};

#[test]
#[ignore = "real Claude Bash, resume and compaction through kah"]
fn mine_claude_resume_and_compact() {
    let f = Fixture::new();
    let version = version();
    let mut terminal = f.start(false);
    f.ready(&mut terminal);
    let goal = "steer: Keep the marker BLUE. Use native Bash to run printf 'BLUE\\n' > marker.txt, then finish.";
    f.prompt(&mut terminal, goal);
    f.wait(&mut terminal, || f.marker("BLUE"));
    f.settled(&mut terminal, goal);
    let first = choices(&f);
    assert!(tools(&f).iter().any(|s| s == "Bash"));
    terminal.submit("/exit");
    terminal.wait_exit(false);
    drop(terminal);
    let mut terminal = f.start(true);
    f.ready(&mut terminal);
    let resumed =
        "Read marker.txt then write resumed.txt with its required color; do not change marker.txt.";
    f.prompt(&mut terminal, resumed);
    f.wait(&mut terminal, || f.has_file("resumed.txt", "BLUE"));
    f.settled(&mut terminal, resumed);
    let restored = choices(&f);
    assert_eq!(
        first["model"], restored["model"],
        "native resume changed model"
    );
    assert!(
        matches!(
            restored["effort"].as_str(),
            Some("low" | "medium" | "high" | "xhigh" | "max")
        ),
        "unsupported native resumed effort: {restored}"
    );
    assert_eq!(restored["effort"], restored["perTurnEffort"]);
    compact(&f, &mut terminal);
    let after =
        "Using the standing marker requirement, create after-compact.txt with the required color.";
    f.prompt(&mut terminal, after);
    f.wait(&mut terminal, || f.has_file("after-compact.txt", "BLUE"));
    f.settled(&mut terminal, after);
    let state = Store::read(&f.base, &f.id).unwrap().unwrap();
    assert!(state.gap.is_none(), "{:?}", state.gap);
    assert!(
        engine::projection(&state)
            .unwrap()
            .unwrap()
            .frontier
            .iter()
            .any(|i| i.text == goal)
    );
    terminal.submit("/exit");
    terminal.wait_exit(false);
    drop(terminal);
    write(
        &f,
        "resume-compact.json",
        json!({"version":version,"passed":true,"nativeBash":true,
        "resumeLaunchModelEffortArgs":false,"beforeChoices":first,"resumedChoices":restored,
        "nativeEffortParity":true,"effortRule":"Claude CLI --effort is current-session only; native supported default on no-override resume",
        "nativeCompactBoundary":true,"colorPreservedAfterResumeAndCompact":true,"observerGap":false}),
    );
}

fn compact(f: &Fixture, terminal: &mut pty::Terminal) {
    let before = Store::read(&f.base, &f.id).unwrap().unwrap().observations;
    terminal.submit("/compact");
    f.wait(terminal, || {
        f.rows()
            .iter()
            .any(|r| r["type"] == "system" && r["subtype"] == "compact_boundary")
    });
    f.wait(terminal, || {
        Store::read(&f.base, &f.id)
            .ok()
            .flatten()
            .is_some_and(|s| s.observations > before && s.inflight.is_none() && s.gap.is_none())
    });
}

#[test]
#[ignore = "one native Claude subagent, explicitly requested by the fixture user"]
fn mine_claude_native_subagent() {
    let f = Fixture::new();
    let version = version();
    let mut terminal = f.start(false);
    f.ready(&mut terminal);
    let task = "Use the native Agent tool to start one Claude Sonnet 5.5 subagent at low effort with this task: write child.txt containing CHILD_OK in this project. Wait for it to finish. Do not write the file yourself. Then reply CHILD_DONE.";
    f.prompt(&mut terminal, task);
    f.wait(&mut terminal, || f.has_file("child.txt", "CHILD_OK"));
    while !claude_support::subagent_completion::completed(&f.rows()) {
        terminal.respond_to_cursor_queries();
        std::thread::sleep(std::time::Duration::from_millis(200));
    }
    f.settled(&mut terminal, task);
    assert!(
        tools(&f)
            .iter()
            .any(|s| matches!(s.as_str(), "Agent" | "Task"))
    );
    assert!(f.child_transcripts() > 0);
    let state = Store::read(&f.base, &f.id).unwrap().unwrap();
    assert!(state.gap.is_none(), "{:?}", state.gap);
    assert_eq!(state.binding.session, f.id);
    let inputs = Store::open(&f.base, &f.id, std::time::Duration::ZERO)
        .unwrap()
        .sources()
        .unwrap();
    assert_eq!(inputs.len(), 1, "subagent output became user source");
    terminal.submit("/exit");
    terminal.wait_exit(false);
    drop(terminal);
    write(
        &f,
        "subagents.json",
        json!({"version":version,"passed":true,"fixtureModel":"claude-sonnet-5-5",
        "nativeAgentTool":true,"childTranscriptCount":f.child_transcripts(),"childFileCreated":true,
        "correlatedChildResultThenParentDoneAndTurnDuration":true,
        "authoritativeUserInputs":inputs.len(),"parentBindingPreserved":true,"observerGap":false}),
    );
}

fn choices(f: &Fixture) -> Value {
    let row = f
        .rows()
        .into_iter()
        .rfind(|r| r["type"] == "assistant")
        .unwrap();
    json!({"model":row["message"]["model"],"effort":row["effort"],"perTurnEffort":row["perTurnEffort"]})
}
fn tools(f: &Fixture) -> Vec<String> {
    f.rows()
        .iter()
        .filter_map(|r| r["message"]["content"].as_array())
        .flatten()
        .filter(|p| p["type"] == "tool_use")
        .filter_map(|p| p["name"].as_str().map(String::from))
        .collect()
}
fn version() -> String {
    let output = Command::new("claude").arg("--version").output().unwrap();
    assert!(output.status.success());
    String::from_utf8(output.stdout).unwrap().trim().into()
}
fn write(f: &Fixture, name: &str, value: Value) {
    fs::write(
        f.evidence.join(name),
        serde_json::to_vec_pretty(&value).unwrap(),
    )
    .unwrap();
}
