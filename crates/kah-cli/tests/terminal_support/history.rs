use kah_codex::{
    control::Control,
    overlay::{Policy, prepare},
};
use serde_json::{Value, json};
use std::{
    path::Path,
    thread,
    time::{Duration, Instant},
};

pub fn connect(home: &Path, project: &Path) -> Control {
    let inspection = home.join(".local/state/kah/codex/control");
    prepare(
        &home.join(".codex"),
        &inspection,
        &Policy {
            restricted: true,
            online: false,
            root: project,
            cwd: project,
        },
    )
    .unwrap();
    Control::connect(
        &home.join(".local/bin/codex"),
        &inspection,
        project,
        Duration::from_secs(10),
    )
    .unwrap()
}

pub fn session(
    client: &mut Control,
    project: &Path,
    terminal: &mut super::pty::Terminal,
) -> String {
    let deadline = Instant::now() + Duration::from_secs(45);
    loop {
        let page = client
            .request(
                "thread/list",
                json!({"cwd":project,"limit":10,"sourceKinds":["cli","appServer"]}),
            )
            .unwrap();
        if let Some(item) = page["data"].as_array().and_then(|v| v.first()) {
            assert_eq!(Path::new(item["cwd"].as_str().unwrap()), project);
            return item["id"].as_str().unwrap().into();
        }
        assert!(Instant::now() < deadline, "native session was not indexed");
        // A cursor-position probe may consume the first Enter while the TUI is
        // still completing terminal initialization. Opening restricted is
        // idempotent; a later Enter in an empty composer submits no prompt.
        terminal.send("\r");
        thread::sleep(Duration::from_secs(1));
    }
}

pub fn completed_turn(client: &mut Control, id: &str, after: Option<&str>) -> Value {
    let deadline = Instant::now() + Duration::from_secs(180);
    loop {
        let page = client
            .request(
                "thread/turns/list",
                json!({"threadId":id,"limit":1,"sortDirection":"desc","itemsView":"full"}),
            )
            .unwrap();
        if let Some(turn) = page["data"].as_array().and_then(|v| v.first())
            && turn["id"].as_str() != after
            && (turn["status"] == "completed" || !turn["completedAt"].is_null())
        {
            assert_eq!(turn["status"], "completed", "{turn}");
            return turn.clone();
        }
        assert!(Instant::now() < deadline, "native TUI turn did not finish");
        thread::sleep(Duration::from_millis(300));
    }
}

/// Cursor-position startup probes can consume the first folder-menu Enter.
/// Empty Enter is idempotent; stop sending as soon as the resumed turn starts.
pub fn resumed_turn(
    client: &mut Control,
    id: &str,
    after: &str,
    terminal: &mut super::pty::Terminal,
) -> Value {
    let deadline = Instant::now() + Duration::from_secs(45);
    loop {
        let page = client
            .request(
                "thread/turns/list",
                json!({"threadId":id,"limit":1,"sortDirection":"desc","itemsView":"full"}),
            )
            .unwrap();
        if page["data"]
            .as_array()
            .and_then(|v| v.first())
            .is_some_and(|v| v["id"].as_str() != Some(after))
        {
            return completed_turn(client, id, Some(after));
        }
        assert!(Instant::now() < deadline, "resumed turn did not start");
        terminal.send("\r");
        thread::sleep(Duration::from_millis(500));
    }
}
