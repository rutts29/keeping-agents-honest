#[path = "../../kah-codex/tests/live_support/client.rs"]
mod client;
#[path = "journeys/fixture.rs"]
mod fixture;
use client::Client;
use kah_core::run_store::RunStore;
use kah_observer::{engine, session::Store};
use serde_json::json;
use std::{
    fs,
    path::{Path, PathBuf},
};

#[test]
#[ignore = "live native observer test; requires provider login, sandbox setup and a TypeSafe key"]
fn automatic_observer_records_user_intent() {
    let home = PathBuf::from(std::env::var_os("HOME").unwrap());
    let before = fs::read(home.join(".codex/config.toml")).unwrap();
    let (launch, mut f) = fixture::prepare(&home, "research", false);
    let mut client = Client::start_command(launch.command(true), &f.stderr);
    let start = client.request("thread/start", json!({"cwd":f.project}));
    let id = start["thread"]["id"].as_str().unwrap();
    f.sessions.push(id.into());
    let turn=client.turn(id,"Create answer.txt containing exactly FIRST. Use native apply_patch, no other tools, dependencies or commits. This task ends when the file exists. Then report and stop.");
    assert_eq!(turn["status"], "completed");
    let base = RunStore::default_state_root().unwrap();
    let state = Store::read(&base, id)
        .unwrap()
        .expect("automatic observer created state");
    assert!(state.gap.is_none(), "{:?}", state.gap);
    assert!(state.run_id.is_some(), "{:?}", state);
    let projection = engine::projection(&state).unwrap().unwrap();
    assert!(
        projection.rendered_prompt().contains("FIRST"),
        "{projection:?}"
    );
    assert_eq!(state.reminders, 0);
    assert_eq!(
        fs::read_to_string(f.project.join("answer.txt"))
            .unwrap()
            .trim(),
        "FIRST"
    );
    assert_config(&home, &before);
    fs::write(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/gates/evidence/phase-3/automatic-smoke.json"),serde_json::to_string_pretty(&json!({"mainAgentUsage":main_usage(&client),"automaticAttachment":true,"userIntentProjection":projection,"usage":state.usage,"reminders":state.reminders,"globalConfigUnchanged":true,"passed":true})).unwrap()+"\n").unwrap();
}

#[path = "observer_journeys/continuity.rs"]
mod continuity;

#[test]
#[ignore = "real steering, correction, scope change, resume and forced compaction; gate only"]
fn steering_survives_resume_and_compaction() {
    continuity::run();
}

#[path = "observer_journeys/interruption.rs"]
mod interruption;

#[test]
#[ignore = "kill automatic observer during concurrent Research/Untrusted sessions; gate only"]
fn observer_kill_fails_open_and_sessions_stay_separate() {
    interruption::run();
}

fn main_usage(client: &Client) -> serde_json::Value {
    client
        .events
        .iter()
        .rev()
        .find(|e| e["method"] == "thread/tokenUsage/updated")
        .map(|e| e["params"]["tokenUsage"].clone())
        .unwrap_or(serde_json::Value::Null)
}

fn assert_backend(state: &kah_observer::session::State) {
    let expected = std::env::var("KAH_TEST_JUDGE").unwrap_or_else(|_| "jev".into());
    assert!(!state.judgments.is_empty());
    assert!(
        state
            .judgments
            .iter()
            .all(|j| j.backend == expected && j.fallback_reason.is_none()),
        "expected {expected}: {:?}",
        state.judgments
    );
}

fn begin_evidence(name: &str) -> PathBuf {
    let backend = std::env::var("KAH_TEST_JUDGE").unwrap_or_else(|_| "jev".into());
    let directory = evidence_directory();
    let path = directory.join(format!("{name}-{backend}.json"));
    fs::write(&path,serde_json::to_string_pretty(&json!({"passed":false,"status":"running","backend":backend,"questionSetVersion":kah_observer::judge::questions::VERSION})).unwrap()+"\n").unwrap();
    path
}

fn assert_config(home: &Path, before: &[u8]) {
    assert!(
        before == fs::read(home.join(".codex/config.toml")).unwrap(),
        "GLOBAL_CONFIG_CHANGED stage=journey"
    );
}

fn evidence_directory() -> PathBuf {
    std::env::var_os("KAH_GATE_EVIDENCE")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/gates/evidence/phase-3")
        })
}
