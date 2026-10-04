//! Native turn completion, rather than a transient Herdr idle report.
use super::*;
use kah_cli::{native_history::History, observer_review::state::Store as Reviews};

pub(super) fn submit(worker: &str, provider: &str, project: &Path, pane: &Pane, text: &str) {
    let home = PathBuf::from(std::env::var_os("HOME").unwrap());
    let mut history = History::open(
        &home,
        &Reviews::directory(worker).unwrap().join("fixture-turn"),
        provider,
        project,
        worker,
    )
    .unwrap();
    let previous = last_turn(&mut history);
    pane.prompt(text).unwrap();
    loop {
        let users = history.snapshot().unwrap().users;
        let seen = users
            .as_ref()
            .is_some_and(|u| u.records().iter().any(|r| r.content() == text));
        let turn = last_turn(&mut history);
        if finished(
            provider,
            &previous,
            &turn,
            seen,
            history.completed().unwrap(),
        ) {
            super::linked_steer::wait_input(worker, text);
            return;
        }
        // 0.160.0's inspection server has no live thread in its own registry;
        // it marks an external in-progress rollout interrupted. Only explicit
        // failure evidence is terminal here; a persisted completion is required.
        assert!(
            turn["error"].is_null(),
            "native fixture turn failed: {turn}"
        );
        std::thread::sleep(Duration::from_millis(250));
    }
}

fn finished(provider: &str, previous: &Value, current: &Value, seen: bool, complete: bool) -> bool {
    seen && complete
        && (provider == "claude"
            || (current["id"] != previous["id"] && current["status"] == "completed"))
}

#[test]
fn an_intake_receipt_does_not_make_the_previous_native_turn_the_new_completion() {
    let previous = json!({"id":"phase-2","status":"completed"});
    assert!(!finished("codex", &previous, &previous, true, true));
    assert!(!finished(
        "codex",
        &previous,
        &json!({"id":"repair","status":"interrupted","completedAt":null,"error":null}),
        true,
        false
    ));
    assert!(!finished(
        "codex",
        &previous,
        &json!({"id":"repair","status":"inProgress"}),
        true,
        false
    ));
    assert!(finished(
        "codex",
        &previous,
        &json!({"id":"repair","status":"completed"}),
        true,
        true
    ));
    assert!(!finished("claude", &Value::Null, &Value::Null, false, true));
    assert!(!finished("claude", &Value::Null, &Value::Null, true, false));
    assert!(finished("claude", &Value::Null, &Value::Null, true, true));
}

fn last_turn(history: &mut History) -> Value {
    if let History::Codex {
        client, session, ..
    } = history
    {
        let page=client.request("thread/turns/list",json!({"threadId":session,"limit":1,"sortDirection":"desc","itemsView":"notLoaded"})).unwrap();
        page["data"]
            .as_array()
            .and_then(|a| a.first())
            .cloned()
            .unwrap_or(Value::Null)
    } else {
        Value::Null
    }
}
