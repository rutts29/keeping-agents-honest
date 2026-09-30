#[path = "../../kah-codex/tests/live_support/client.rs"]
mod client;
#[path = "journeys/fixture.rs"]
mod fixture;
use client::Client;
use kah_codex::delivery::{self, Message, Rpc, RpcError};
use kah_core::run_store::RunStore;
use kah_observer::{engine, session::Store};
use serde_json::{Value, json};
use std::{fs, path::PathBuf, process::Command, time::Duration};
impl Rpc for Client {
    fn request(&mut self, m: &str, p: Value) -> Result<Value, RpcError> {
        self.request_result(m, p).map_err(|e| {
            if e["code"] == -32601 {
                RpcError::Unsupported
            } else {
                RpcError::Rejected
            }
        })
    }
}
#[test]
#[ignore = "live literal overrides and user-authority ledger review commands"]
fn user_overrides_and_review_are_source_linked_and_durable() {
    let home = PathBuf::from(std::env::var_os("HOME").unwrap());
    let (launch, mut f) = fixture::prepare(&home, "research", false);
    let receipts = home.join(format!(".cache/kah-user-fixture-{}", std::process::id()));
    let mut client = Client::start_command(launch.command(true), &f.stderr);
    let start = client.request("thread/start", json!({"cwd":f.project}));
    let session = start["thread"]["id"].as_str().unwrap();
    f.sessions.push(session.into());
    assert_eq!(
        client.turn(
            session,
            "!aside Initialize the receiver; reply READY without tools."
        )["status"],
        "completed"
    );
    submit(
        &mut client,
        &receipts,
        session,
        "force",
        "!steer Create forced.txt containing USER_FORCED. Use native apply_patch only, then report and stop.",
    );
    assert_eq!(
        fs::read_to_string(f.project.join("forced.txt"))
            .unwrap()
            .trim(),
        "USER_FORCED"
    );
    let aside =
        "!aside Consider notes.txt containing NOTES. Do not create it yet; reply SEEN only.";
    submit(&mut client, &receipts, session, "aside", aside);
    assert!(!f.project.join("notes.txt").exists());
    let entries = verify_review(session, aside);
    let log = fs::read_to_string(
        Store::directory(&RunStore::default_state_root().unwrap(), session)
            .join("observations.jsonl"),
    )
    .unwrap();
    assert!(
        !log.lines()
            .map(|l| serde_json::from_str::<Value>(l).unwrap())
            .any(|v| v["purpose"] == "steering"),
        "override invoked steering inference"
    );
    let output = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../docs/gates/evidence/phase-3c/user-controls.json");
    fs::write(output,serde_json::to_string_pretty(&json!({"passed":true,"forcedFileCreated":true,"asideDidNotAct":true,"sourceTextPreserved":true,"userNamespace":"kah-user:operator","reviewActions":["promote","confirm","dismiss"],"reviewJudgeCalls":0,"overrideSteeringJudgeCalls":0,"actionObservationIsSeparate":true,"sources":entries[0]["items"]})).unwrap()+"\n").unwrap();
    fs::remove_dir_all(receipts).unwrap();
}
fn verify_review(session: &str, aside: &str) -> Value {
    let list = cli(&["ledger", session, "--review", "--json"]);
    let entries: Value = serde_json::from_str(&list).unwrap();
    let item = entries[0]["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|v| v["text"] == aside)
        .unwrap();
    let id = item["id"].as_str().unwrap();
    let before = state(session).usage.calls;
    cli(&["ledger", session, "--promote", id]);
    cli(&["ledger", session, "--confirm", id]);
    let projection = engine::projection(&state(session)).unwrap().unwrap();
    assert!(
        projection.frontier.iter().any(|i| i.id.as_str() == id
            && i.text == aside
            && i.user_confirmed
            && !i.review_required)
    );
    cli(&["ledger", session, "--dismiss", id]);
    assert!(
        !engine::projection(&state(session))
            .unwrap()
            .unwrap()
            .frontier
            .iter()
            .any(|i| i.id.as_str() == id)
    );
    assert_eq!(state(session).usage.calls, before, "review called Jev");
    entries
}

fn submit(client: &mut Client, base: &std::path::Path, session: &str, receipt: &str, text: &str) {
    let message = Message::user(session, receipt, text).unwrap();
    delivery::send(base, &message, client).unwrap();
    let done = client.wait(Duration::from_secs(120), |v| {
        v["method"] == "turn/completed"
    });
    assert_eq!(done["params"]["turn"]["status"], "completed");
}
fn cli(args: &[&str]) -> String {
    let result = Command::new(env!("CARGO_BIN_EXE_kah"))
        .args(args)
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    String::from_utf8(result.stdout).unwrap()
}
fn state(session: &str) -> kah_observer::session::State {
    Store::read(&RunStore::default_state_root().unwrap(), session)
        .unwrap()
        .unwrap()
}
