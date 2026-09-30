#[path = "../../kah-codex/tests/live_support/client.rs"]
mod client;
#[path = "journeys/fixture.rs"]
mod fixture;
use client::Client;
use kah_codex::delivery::{self, Message, Rpc, RpcError};
use serde_json::{Value, json};
use std::{fs, path::PathBuf, time::Duration};
impl Rpc for Client {
    fn request(&mut self, method: &str, params: Value) -> Result<Value, RpcError> {
        self.request_result(method, params).map_err(|e| {
            if e["code"] == -32601 {
                RpcError::Unsupported
            } else {
                RpcError::Rejected
            }
        })
    }
}
#[test]
#[ignore = "native delegate sender receipts, active delivery, restart and authority isolation"]
fn delegate_sender_retries_and_restarts_without_duplicate_steering() {
    let home = PathBuf::from(std::env::var_os("HOME").unwrap());
    let (launch, mut fixture) = fixture::prepare(&home, "research", false);
    let receipts = home.join(format!(".cache/kah-send-fixture-{}", std::process::id()));
    let mut client = Client::start_command(launch.command(true), &fixture.stderr);
    let start = client.request("thread/start", json!({"cwd":fixture.project}));
    let session = start["thread"]["id"].as_str().unwrap();
    fixture.sessions.push(session.into());
    let setup = client.turn(
        session,
        "aside: Initialize this receiver; reply READY without tools.",
    );
    assert_eq!(setup["status"], "completed");
    let first = Message::new(
        session,
        "kah-reviewer",
        "start",
        "Run native sleep 5, then reply DONE. Do nothing else.",
    )
    .unwrap();
    let second = Message::new(
        session,
        "kah-reviewer",
        "steer",
        "After sleeping reply SOURCE_MARKED. Do nothing else.",
    )
    .unwrap();
    delivery::send(&receipts, &first, &mut client).unwrap();
    delivery::send(&receipts, &second, &mut client).unwrap();
    client.wait(Duration::from_secs(120), |v| {
        v["method"] == "turn/completed"
    });
    drop(client);
    let mut resumed = Client::start_command(launch.command(true), &fixture.stderr);
    resumed.request("thread/resume",json!({"threadId":session,"cwd":fixture.project,"permissions":":danger-full-access","approvalPolicy":"never"}));
    let retried = delivery::send(&receipts, &second, &mut resumed).unwrap();
    assert_eq!(retried["status"], "already_visible");
    let messages = verify_messages(&mut resumed, session);
    let state = kah_observer::session::Store::read(
        &kah_core::run_store::RunStore::default_state_root().unwrap(),
        session,
    )
    .unwrap()
    .unwrap();
    assert!(state.gap.is_none(), "{:?}", state.gap);
    assert!(state.run_id.is_none(), "delegate became user intent");
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../docs/gates/evidence/phase-3c/delegate-delivery.json");
    fs::write(path,serde_json::to_string_pretty(&json!({"passed":true,"messages":messages,"retry":retried,"onlyTwoDelegateMessagesAfterRestart":true,"noUserIntentFromDelegate":true})).unwrap()+"\n").unwrap();
    fs::remove_dir_all(receipts).unwrap();
}

fn verify_messages(resumed: &mut Client, session: &str) -> Vec<Value> {
    let read = resumed.request(
        "thread/read",
        json!({"threadId":session,"includeTurns":true}),
    );
    let messages = read["thread"]["turns"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|t| t["items"].as_array().unwrap())
        .filter(|i| i["type"] == "userMessage")
        .cloned()
        .collect::<Vec<_>>();
    let delegates = messages
        .iter()
        .filter(|m| {
            m["clientId"]
                .as_str()
                .is_some_and(|s| s.starts_with("kah-delegate:"))
        })
        .collect::<Vec<_>>();
    assert_eq!(delegates.len(), 2);
    assert!(
        delegates
            .iter()
            .all(|m| kah_codex::authorship::with_client_id(
                m["content"][0]["text"].as_str().unwrap(),
                m["clientId"].as_str()
            ) == kah_core::intent_history::SourceRole::Delegate)
    );

    messages
}

#[test]
#[ignore = "read-only native empty-thread compatibility diagnostic; no model call"]
fn empty_thread_metadata() {
    let home = PathBuf::from(std::env::var_os("HOME").unwrap());
    let (launch, mut f) = fixture::prepare(&home, "research", false);
    let mut client = Client::start_command(launch.command(true), &f.stderr);
    let start = client.request("thread/start", json!({"cwd":f.project}));
    let id = start["thread"]["id"].as_str().unwrap();
    f.sessions.push(id.into());
    for include in [false, true] {
        let result =
            client.request_result("thread/read", json!({"threadId":id,"includeTurns":include}));
        match result {
            Ok(v) => eprintln!(
                "includeTurns={include} status={} previewEmpty={} turns={}",
                v["thread"]["status"],
                v["thread"]["preview"].as_str().is_some_and(str::is_empty),
                v["thread"]["turns"]
            ),
            Err(e) => eprintln!("includeTurns={include} error={e}"),
        }
    }
}
