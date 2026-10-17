#[allow(dead_code)]
mod claude_support;
#[path = "claude_support/observations.rs"]
mod observations;
#[allow(dead_code)]
#[path = "terminal_support/processes.rs"]
mod processes;
#[allow(dead_code)]
#[path = "terminal_support/pty.rs"]
mod pty;

use claude_support::Fixture;
use kah_observer::{engine, session::Store};
use observations::*;
use serde_json::json;
use std::time::Instant;

#[test]
#[ignore = "real Claude correction, scope change and Jev metrics"]
fn claude_correction_and_scope() {
    let f = Fixture::new();
    let version = version();
    let started = Instant::now();
    let mut terminal = f.start(false);
    f.ready(&mut terminal);
    let initial = "Maintain a disposable marker fixture and follow my upcoming file instructions. No dependencies, subagents, installs, tests or commits. Reply READY without tools.";
    f.prompt(&mut terminal, initial);
    f.settled(&mut terminal, initial);
    let alpha = "Use uppercase marker ALPHA in answer.txt. Create it now with native Write, preserve other constraints, report and stop.";
    f.prompt(&mut terminal, alpha);
    f.wait(&mut terminal, || f.has_file("answer.txt", "ALPHA"));
    f.settled(&mut terminal, alpha);
    let correction = "Correction: BETA replaces ALPHA as the required marker for this entire session. Update answer.txt to BETA now. The earlier ALPHA instruction is superseded. Keep other constraints.";
    f.prompt(&mut terminal, correction);
    f.wait(&mut terminal, || f.has_file("answer.txt", "BETA"));
    f.settled(&mut terminal, correction);
    let scope = "Intentional scope change: also maintain notes.txt containing the same current marker as answer.txt. Add notes.txt now and preserve answer.txt. This extra file is explicitly authorized.";
    f.prompt(&mut terminal, scope);
    f.wait(&mut terminal, || f.has_file("notes.txt", "BETA"));
    f.settled(&mut terminal, scope);
    let state = state(&f);
    let projection = engine::projection(&state).unwrap().unwrap();
    let active = projection
        .preserve
        .iter()
        .chain(&projection.frontier)
        .chain(&projection.validation)
        .map(|i| i.text.as_str())
        .collect::<Vec<_>>();
    assert_eq!(projection.goal.text, initial);
    assert!(!active.contains(&alpha));
    assert!(active.contains(&correction));
    assert!(active.contains(&scope));
    assert_eq!(state.reminders, 0);
    finish(&mut terminal);
    write(
        &f,
        "correction-scope.json",
        json!({"version":version,"passed":true,"fixtureModel":"claude-sonnet-5-5",
        "originalGoalVerbatim":true,"obsoleteInstructionReplaced":true,"correctionAndScopeVerbatim":true,
        "scopeChangeReminders":state.reminders,"observerUsage":state.usage,"judgments":state.judgments.len(),
        "mainModelUsage":native_usage(&f),"wallElapsedMs":started.elapsed().as_millis()}),
    );
}

#[test]
#[ignore = "kill only the fixture's stamped observer process; native Claude must continue"]
fn claude_observer_interruption() {
    let f = Fixture::new();
    let version = version();
    let mut terminal = f.start(false);
    f.ready(&mut terminal);
    let task = "Create interrupted.txt containing BEFORE_KILL with native Write. No other tools or files; then report and stop.";
    f.prompt(&mut terminal, task);
    let (killed, held) = kill_observer(&f);
    let gap = gap_status(&f);
    drop(held);
    f.wait(&mut terminal, || {
        f.has_file("interrupted.txt", "BEFORE_KILL")
    });
    f.settled(&mut terminal, task);
    let next = "Also create continued.txt containing WORKING using native Write. Keep interrupted.txt unchanged, then stop.";
    f.prompt(&mut terminal, next);
    f.wait(&mut terminal, || f.has_file("continued.txt", "WORKING"));
    f.settled(&mut terminal, next);
    let state = state(&f);
    assert!(
        std::fs::read_to_string(Store::directory(&f.base, &f.id).join("observations.jsonl"))
            .unwrap()
            .contains("OBSERVER_INTERRUPTED")
    );
    finish(&mut terminal);
    write(
        &f,
        "interruption.json",
        json!({"version":version,"passed":true,"fixtureModel":"claude-sonnet-5-5",
        "observerKilled":killed,"gapVisibleInStatus":gap,"mainSessionContinued":true,"recoveredGap":state.gap,
        "observerUsage":state.usage,"interruptedRequestUsageMayBeIncomplete":true}),
    );
}
