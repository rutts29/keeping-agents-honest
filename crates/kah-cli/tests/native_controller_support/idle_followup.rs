//! A completed native reply for an active follow-up round; no model or Jev call.
use super::*;
use crate::observer_review::gate_approval::followup::{Binding, Stage, VerificationRound};
use serde_json::{Value, json};
use std::fs;

const SESSION: &str = "5e55a1a0-0000-4000-8000-0000000000b2";

/// A worker whose controller state has no tool action digest.
fn state(project: &std::path::Path) -> session::State {
    session::State::new(session::Binding {
        session: SESSION.into(),
        project: project.into(),
        codex_home: project.join("codex"),
        native: "claude".into(),
        model: "claude-sonnet-5-5".into(),
        effort: "low".into(),
        timeout_secs: 45,
        judge: Default::default(),
        jev_model: "jev-latest".into(),
    })
}
/// The active builder round, its id derived from its binding.
fn round() -> VerificationRound {
    let binding = Binding {
        worker: SESSION.into(),
        review: "review-1700000500000-501".into(),
        head: "6d1c9b8a7f6e5d4c3b2a1f0e9d8c7b6a5f4e3d21".into(),
        intent_revision: 2,
    };
    VerificationRound {
        id: binding.id().unwrap(),
        binding,
        stage: Stage::Builder,
        claimed: true,
        evidence: Some(vec![]),
        builder: None,
        checks: None,
    }
}
/// The worker received the round, replied with its receipt and ended the turn.
fn transcript(receipt: &str, project: &std::path::Path) -> String {
    let row = |mut row: Value| {
        row["cwd"] = json!(project);
        format!("{row}\n")
    };
    [
        row(json!({"type":"user","uuid":"00000000-0000-4000-8000-0000000000c1","sessionId":SESSION,
            "isSidechain":false,"message":{"role":"user","content":"Run the follow-up checks and reply with the receipt."}})),
        row(json!({"type":"assistant","uuid":"00000000-0000-4000-8000-0000000000c2","sessionId":SESSION,
            "isSidechain":false,"message":{"role":"assistant","stop_reason":"end_turn",
            "content":[{"type":"text","text":format!("Checks ran and passed. Receipt: {receipt}")}]}})),
        row(json!({"type":"system","subtype":"turn_duration","uuid":"00000000-0000-4000-8000-0000000000c3",
            "sessionId":SESSION,"isSidechain":false,"durationMs":4000})),
    ]
    .concat()
}

#[test]
fn idle_completed_reply_rekicks_once_without_a_tool_action_digest() {
    std::env::var_os("XDG_STATE_HOME").expect("isolated check-tests fixture required");
    let store = crate::observer_review::state::Store::open(SESSION).unwrap();
    let root = store.directory.clone();
    let state = state(&root.join("project"));
    assert!(state.action_digest.is_none());
    let round = round();
    let rounds = root.join("gate-followups");
    crate::unattended_report::save(
        &rounds,
        &format!("{}.json", round.id),
        &serde_json::to_value(&round).unwrap(),
    )
    .unwrap();
    crate::unattended_report::save(&rounds.join(&round.id), "job.json", &json!({})).unwrap();
    let path = root.join("worker.jsonl");
    fs::write(
        &path,
        transcript(&round.builder_receipt(), &root.join("project")),
    )
    .unwrap();
    let mut history = History::Claude {
        path,
        project: root.join("project"),
        session: SESSION.into(),
    };
    assert!(history.completed().unwrap());
    let mut tracker = Tracker::default();
    let mut kicks = 0;
    poll(&state, &mut history, &mut tracker, || {
        kicks += 1;
        Ok(())
    })
    .unwrap();
    assert_eq!(
        kicks, 1,
        "the complete receipt reply must wake the existing round"
    );
    poll(&state, &mut history, &mut tracker, || {
        kicks += 1;
        Ok(())
    })
    .unwrap();
    assert_eq!(
        kicks, 1,
        "unchanged completed progress must not duplicate a wake"
    );
    completion_history(&state, &mut history);
    drop(store);
    fs::remove_dir_all(root.parent().unwrap()).unwrap();
}

fn completion_history(state: &session::State, history: &mut History) {
    use std::io::Write;
    let captured = history.snapshot().unwrap();
    let fresh = history.snapshot().unwrap();
    assert!(
        super::super::completion::unchanged_history(state, &captured, &fresh),
        "NULL legacy state must not reject genuinely unchanged native history"
    );
    let History::Claude { path, session, .. } = history else {
        unreachable!()
    };
    // A later assistant message changes the native work.
    let changed = json!({"type":"assistant","uuid":"00000000-0000-4000-8000-000000000001",
        "sessionId":session,"isSidechain":false,"test_control":true,
        "message":{"role":"assistant","content":[{"type":"text","text":"Control-only negative: native work changed."}]}});
    let mut file = fs::OpenOptions::new().append(true).open(path).unwrap();
    writeln!(file, "{changed}").unwrap();
    let fresh = history.snapshot().unwrap();
    assert!(
        !super::super::completion::unchanged_history(state, &captured, &fresh),
        "actual changed source content must stay incomplete"
    );
}
