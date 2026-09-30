use kah_codex::{
    control::Control,
    overlay::{self, Policy},
};
use kah_core::run_store::RunStore;
use serde_json::{Value, json};
use std::{fs, path::PathBuf, time::Duration};
#[test]
#[ignore = "native history after completed real worker journey; gate only"]
fn worker_threads_are_absent_from_native_resume_history() {
    let home = PathBuf::from(std::env::var_os("HOME").unwrap());
    let evidence =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../docs/gates/evidence/phase-4");
    let complete: Value =
        serde_json::from_slice(&fs::read(evidence.join("complete.json")).unwrap()).unwrap();
    assert_eq!(complete["passed"], true);
    let id = complete["report"]["run"].as_str().unwrap();
    let logs =
        kah_run::commands::logs(&RunStore::default_state_root().unwrap().join("runs"), id).unwrap();
    let ids = logs
        .lines()
        .filter_map(|line| serde_json::from_str::<Value>(line).ok())
        .filter(|v| v["type"] == "thread.started")
        .map(|v| v["thread_id"].as_str().unwrap().to_owned())
        .collect::<Vec<_>>();
    assert!(!ids.is_empty());
    let cwd = std::env::current_dir().unwrap();
    let directory = home.join(".local/state/kah/codex/control");
    overlay::prepare(
        &home.join(".codex"),
        &directory,
        &Policy {
            restricted: true,
            online: false,
            root: &cwd,
            cwd: &cwd,
        },
    )
    .unwrap();
    let mut client = Control::connect(
        &home.join(".local/bin/codex"),
        &directory,
        &cwd,
        Duration::from_secs(10),
    )
    .unwrap();
    let page = client
        .request(
            "thread/list",
            json!({"limit":100,"sortKey":"updated_at","sortDirection":"desc","archived":false}),
        )
        .unwrap();
    for id in &ids {
        assert!(
            !page["data"]
                .as_array()
                .unwrap()
                .iter()
                .any(|t| t["id"] == *id)
        );
        let result = client.request("thread/read", json!({"threadId":id,"includeTurns":false}));
        assert!(
            result.is_err(),
            "ephemeral worker has a resumable stored thread: {id}"
        );
    }
    fs::write(evidence.join("worker-history.json"), serde_json::to_string_pretty(&json!({"passed":true,"workerIds":ids,"absentFromResumeList":true,"noStoredRollout":true,"mechanism":"native codex exec --ephemeral"})).unwrap()+"\n").unwrap();
}
