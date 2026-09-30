#[path = "../../kah-codex/tests/live_support/client.rs"]
mod client;
#[path = "journeys/fixture.rs"]
mod fixture;
use client::Client;
use serde_json::{Value, json};
use std::{fs, path::PathBuf, time::Duration};

#[test]
#[ignore = "live Research/Untrusted Herdr control-plane isolation"]
fn restricted_executors_cannot_reach_herdr() {
    let home = PathBuf::from(std::env::var_os("HOME").unwrap());
    let before = fs::read(home.join(".codex/config.toml")).unwrap();
    let mut reports = vec![];
    for domain in ["research", "untrusted"] {
        let (launch, mut f) = fixture::prepare(&home, domain, false);
        let mut command = launch.command(true);
        command.env("HERDR_BOUNDARY_CANARY", "host-only");
        let mut client = Client::start_command(command, &f.stderr);
        let start = client.request("thread/start", json!({"cwd":f.project}));
        let id = start["thread"]["id"].as_str().unwrap();
        f.sessions.push(id.into());
        let turn=client.turn(id,"Run exactly this native shell command and report its result; no other work: set -eu; test ! -e /home/example/.config/herdr; test ! -e /home/example/.codex/app-server-control; test ! -e /home/example/.local/bin/herdr; ! command -v herdr; ! env | sed 's/=.*//' | grep '^HERDR_'; printf 'HERDR_ISOLATED\\n'");
        assert_eq!(turn["status"], "completed");
        let commands = client
            .events
            .iter()
            .filter(|v| {
                v["method"] == "item/completed" && v["params"]["item"]["type"] == "commandExecution"
            })
            .map(|v| v["params"]["item"].clone())
            .collect::<Vec<_>>();
        assert!(
            commands.iter().any(|v| v["exitCode"] == 0
                && v["aggregatedOutput"]
                    .as_str()
                    .is_some_and(|s| s.contains("HERDR_ISOLATED"))),
            "{commands:?}"
        );
        reports.push(json!({"domain":domain,"commands":commands,"passed":true}));
    }
    assert_eq!(before, fs::read(home.join(".codex/config.toml")).unwrap());
    save(
        "herdr-isolation",
        json!({"reports":reports,"globalConfigUnchanged":true,"mine":"Host access accepted in the single-user trust model","passed":true}),
    );
}

#[test]
#[ignore = "timeboxed native turn/start + turn/steer source-ID persistence probe"]
fn native_client_message_ids_survive_restart() {
    let home = PathBuf::from(std::env::var_os("HOME").unwrap());
    let (launch, mut f) = fixture::prepare(&home, "research", false);
    let mut client = Client::start_command(launch.command(true), &f.stderr);
    let start = client.request("thread/start", json!({"cwd":f.project}));
    let id = start["thread"]["id"].as_str().unwrap();
    f.sessions.push(id.into());
    let turn=client.request("turn/start",json!({"threadId":id,"clientUserMessageId":"kah-delegate:probe:start","input":[{"type":"text","text":"Run native sleep 5 and then reply DONE. Do nothing else."}]}));
    client.request("turn/steer",json!({"threadId":id,"expectedTurnId":turn["turn"]["id"],"clientUserMessageId":"kah-delegate:probe:steer","input":[{"type":"text","text":"After the sleep, reply DELEGATE_PROBE_FINISHED. Do nothing else."}]}));
    client.wait(Duration::from_secs(120), |v| {
        v["method"] == "turn/completed"
    });
    drop(client);
    let mut resumed = Client::start_command(launch.command(true), &f.stderr);
    resumed.request("thread/resume",json!({"threadId":id,"cwd":f.project,"permissions":":danger-full-access","approvalPolicy":"never"}));
    let read = resumed.request("thread/read", json!({"threadId":id,"includeTurns":true}));
    let ids = read["thread"]["turns"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|t| t["items"].as_array().unwrap())
        .filter(|i| i["type"] == "userMessage")
        .map(|i| i["clientId"].clone())
        .collect::<Vec<_>>();
    assert!(ids.contains(&json!("kah-delegate:probe:start")), "{ids:?}");
    assert!(ids.contains(&json!("kah-delegate:probe:steer")), "{ids:?}");
    save(
        "native-message-source",
        json!({"clientIdsAfterRestart":ids,"methods":["turn/start","turn/steer"],"persisted":true,"passed":true}),
    );
}
fn save(name: &str, value: Value) {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join(format!("../../docs/gates/evidence/phase-3c/{name}.json"));
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, serde_json::to_string_pretty(&value).unwrap() + "\n").unwrap();
}
