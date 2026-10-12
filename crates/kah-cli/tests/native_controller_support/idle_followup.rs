//! Retained VF13 complete worker history; no model or Jev call.
use super::*;
use serde_json::{Value, json};
use std::fs;

#[test]
fn retained_idle_completed_reply_rekicks_once_without_a_tool_action_digest() {
    std::env::var_os("XDG_STATE_HOME").expect("isolated check-tests fixture required");
    let control: Value = serde_json::from_str(include_str!(
        "../../../../docs/gates/evidence/phase3-idle-followup/retained-control.json"
    ))
    .unwrap();
    let state: session::State = serde_json::from_value(control["state"].clone()).unwrap();
    assert!(state.action_digest.is_none());
    let store = crate::observer_review::state::Store::open(&state.binding.session).unwrap();
    let root = store.directory.clone();
    let rounds = root.join("gate-followups");
    let round = &control["round"];
    crate::unattended_report::save(
        &rounds,
        &format!("{}.json", round["id"].as_str().unwrap()),
        round,
    )
    .unwrap();
    crate::unattended_report::save(
        &rounds.join(round["id"].as_str().unwrap()),
        "job.json",
        &json!({"retained_control_only":true}),
    )
    .unwrap();
    let path = root.join("worker.jsonl");
    fs::write(
        &path,
        include_bytes!(
            "../../../../docs/gates/evidence/phase3-idle-followup/retained-worker.jsonl"
        ),
    )
    .unwrap();
    let mut history = History::Claude {
        path,
        project: control["project"].as_str().unwrap().into(),
        session: state.binding.session.clone(),
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
    // Explicit synthetic negative in the test COPY, never the retained source.
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
