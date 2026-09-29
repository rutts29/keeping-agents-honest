use crate::client::Client;
use serde_json::{Value, json};
use std::{
    fs,
    path::{Path, PathBuf},
};

mod domains;
mod lifecycle;
mod online_resume;
pub use lifecycle::resume_and_loss;
mod fixture;
pub use domains::all_domains;
use fixture::prepare;

pub fn research() {
    let home = PathBuf::from(std::env::var_os("HOME").unwrap());
    let before = fs::read(home.join(".codex/config.toml")).unwrap();
    let (launch, f) = prepare(&home, "research", false);
    let initial_commit = baseline(&home, &f);
    let mut client = Client::start_command(launch.command(true), &f.stderr);
    let start = client.request("thread/start", json!({"cwd":f.project,"ephemeral":true}));
    assert_eq!(start["model"], "gpt-6-sol");
    assert_eq!(start["reasoningEffort"], "low");
    let config = client.request(
        "config/read",
        json!({"cwd":f.project,"includeLayers":false}),
    );
    assert_eq!(config["config"]["web_search"], "disabled");
    let turn = client.turn(start["thread"]["id"].as_str().unwrap(), &format!("Run this disposable acceptance test using native tools only, no MCP or subagents. Use exec_command once to run: pwd; test ! -e '{}' && echo HOST_HIDDEN; curl --max-time 8 -Is https://example.com; echo CURL_EXIT=$?. Then native apply_patch must create journey.txt with exactly 'native edit works'. Report and stop.",f.canary.display()));
    assert_eq!(turn["status"], "completed");
    let operations = operations(&client.events);
    let commands = operations
        .iter()
        .filter(|v| v["type"] == "commandExecution")
        .map(|v| v["aggregatedOutput"].as_str().unwrap_or(""))
        .collect::<String>();
    assert!(commands.contains("HOST_HIDDEN"), "{operations:?}");
    assert!(commands.contains("CURL_EXIT=6"), "{operations:?}");
    assert!(
        commands.contains(f.project.to_str().unwrap()),
        "{operations:?}"
    );
    assert_eq!(
        fs::read_to_string(f.project.join("journey.txt"))
            .unwrap()
            .trim(),
        "native edit works"
    );
    assert_eq!(fs::read_to_string(&f.canary).unwrap(), "untouched");
    drop(client);
    assert_eq!(fs::read(home.join(".codex/config.toml")).unwrap(), before);
    let evidence = json!({"operations":operations,"model":start["model"],"effort":start["reasoningEffort"],"globalConfigUnchanged":true,"hostCanaryUnchanged":true,"initialCommit":initial_commit,"newProjectCommitted":true,"sharedLoginAndHistory":true,"passed":true});
    let output = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../docs/gates/evidence/phase-2/launcher-research.json");
    fs::write(
        output,
        serde_json::to_string_pretty(&evidence).unwrap() + "\n",
    )
    .unwrap();
}

fn baseline(home: &Path, f: &fixture::Fixture) -> String {
    for name in ["auth.json", "sessions"] {
        assert_eq!(
            fs::canonicalize(f.overlay.join(name)).unwrap(),
            fs::canonicalize(home.join(".codex").join(name)).unwrap()
        );
    }
    let head = std::process::Command::new("git")
        .arg("-C")
        .arg(&f.project)
        .args(["rev-parse", "--verify", "HEAD"])
        .output()
        .unwrap();
    assert!(head.status.success());
    let clean = std::process::Command::new("git")
        .arg("-C")
        .arg(&f.project)
        .args(["status", "--porcelain"])
        .output()
        .unwrap();
    assert!(clean.status.success() && clean.stdout.is_empty());
    assert!(
        fs::read_to_string(f.project.join("AGENTS.md"))
            .unwrap()
            .contains("Dependency decisions")
    );
    String::from_utf8(head.stdout).unwrap().trim().into()
}

fn operations(events: &[Value]) -> Vec<Value> {
    events
        .iter()
        .filter(|v| v["method"] == "item/completed")
        .map(|v| v["params"]["item"].clone())
        .filter(|v| matches!(v["type"].as_str(), Some("commandExecution" | "fileChange")))
        .collect()
}
