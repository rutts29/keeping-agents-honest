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
use serde_json::json;
use std::{fs, process::Command};

#[test]
#[ignore = "isolated native trust/startup fixture; no model turn"]
fn mine_claude_startup() {
    let fixture = Fixture::new();
    let version = Command::new("claude").arg("--version").output().unwrap();
    let mut terminal = fixture.start(false);
    fixture.ready(&mut terminal);
    let state = Store::read(&fixture.base, &fixture.id).unwrap().unwrap();
    assert!(state.gap.is_none());
    terminal.submit("/exit");
    terminal.wait_exit(false);
    drop(terminal);
    fs::write(
        fixture.evidence.join("startup.json"),
        serde_json::to_vec_pretty(&json!({
        "version":String::from_utf8(version.stdout).unwrap().trim(),"passed":true,
        "nativeTrustAccepted":true,"observerReady":true,"modelTurns":0}))
        .unwrap(),
    )
    .unwrap();
}

#[test]
#[ignore = "real Claude TUI and Jev hooks; run only through make gate"]
fn mine_claude_observer_and_native_edits() {
    let fixture = Fixture::new();
    let version = Command::new("claude").arg("--version").output().unwrap();
    let mut terminal = fixture.start(false);
    fixture.ready(&mut terminal);
    fixture.prompt(
        &mut terminal,
        "Use the Write tool to create marker.txt containing FIRST, then reply READY.",
    );
    fixture.wait(&mut terminal, || fixture.marker("FIRST"));
    fixture.settled(&mut terminal, "FIRST");
    let steer = "steer: Use BLUE for the marker. Update marker.txt to BLUE using native Write.";
    fixture.prompt(&mut terminal, steer);
    fixture.wait(&mut terminal, || fixture.marker("BLUE"));
    fixture.settled(&mut terminal, steer);
    let aside = "aside: This is commentary only. Reply ASIDE_OK without edits.";
    fixture.prompt(&mut terminal, aside);
    fixture.settled(&mut terminal, aside);
    let state = Store::read(&fixture.base, &fixture.id).unwrap().unwrap();
    assert!(state.gap.is_none(), "{:?}", state.gap);
    let projection = engine::projection(&state).unwrap().unwrap();
    assert!(projection.frontier.iter().any(|i| i.text == steer));
    assert!(!projection.frontier.iter().any(|i| i.text == aside));
    assert!(state.judgments.iter().all(|j| j.backend == "jev"));
    assert!(!state.judgments.is_empty());
    terminal.submit("/exit");
    terminal.wait_exit(false);
    drop(terminal);
    fs::write(fixture.evidence.join("mine-observer.json"), serde_json::to_vec_pretty(&json!({
        "version":String::from_utf8(version.stdout).unwrap().trim(),"fixtureModel":"claude-sonnet-5-5","fixtureEffort":"low",
        "realTui":true,"nativeWrite":true,"verbatimSteering":true,"asideExcluded":true,
        "judge":"jev","judgeCalls":state.judgments.len(),"observerGap":false,
        "resume":"not yet proven","subagents":"not yet proven"})).unwrap()).unwrap();
}
