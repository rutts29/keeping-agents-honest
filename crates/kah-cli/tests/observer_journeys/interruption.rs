use super::*;
use kah_observer::session::State;
use serde_json::Value;
use std::{
    process::Command,
    thread,
    time::{Duration, Instant},
};

pub fn run() {
    let home = PathBuf::from(std::env::var_os("HOME").unwrap());
    let before = fs::read(home.join(".codex/config.toml")).unwrap();
    let (launch_a, mut a) = fixture::prepare(&home, "research", false);
    let (launch_b, mut b) = fixture::prepare(&home, "untrusted", false);
    let mut first = Client::start_command(launch_a.command(true), &a.stderr);
    let mut second = Client::start_command(launch_b.command(true), &b.stderr);
    let start_a = first.request("thread/start", json!({"cwd":a.project}));
    let start_b = second.request("thread/start", json!({"cwd":b.project}));
    let id_a = start_a["thread"]["id"].as_str().unwrap();
    a.sessions.push(id_a.into());
    let id_b = start_b["thread"]["id"].as_str().unwrap();
    b.sessions.push(id_b.into());
    start(
        &mut first,
        id_a,
        "Create marker.txt containing RESEARCH_ONLY. Use only native apply_patch, no tests, installs or commits. Report and stop.",
    );
    start(
        &mut second,
        id_b,
        "Create marker.txt containing UNTRUSTED_ONLY. Use only native apply_patch, no tests, installs or commits. Report and stop.",
    );
    finish(&mut first, id_a);
    finish(&mut second, id_b);
    assert!(state(id_a).run_id.is_some() && state(id_b).run_id.is_some());
    start(
        &mut first,
        id_a,
        "Also create interrupted.txt containing BEFORE_KILL. Keep marker.txt unchanged; use native apply_patch only, then stop.",
    );
    let killed = kill_observer(id_a);
    finish(&mut first, id_a);
    let status = gap_status(id_a);
    let turn=first.turn(id_a,"Keep RESEARCH_ONLY in marker.txt and also create continued.txt containing WORKING. Use native apply_patch only, then stop.");
    assert_eq!(turn["status"], "completed");
    let state_a = state(id_a);
    let state_b = state(id_b);
    assert!(state_b.gap.is_none(), "{state_b:?}");
    let projection_a = engine::projection(&state_a).unwrap().unwrap();
    let projection_b = engine::projection(&state_b).unwrap().unwrap();
    assert!(!projection_a.rendered_prompt().contains("UNTRUSTED_ONLY"));
    assert!(!projection_b.rendered_prompt().contains("RESEARCH_ONLY"));
    assert!(projection_a.rendered_prompt().contains("RESEARCH_ONLY"));
    assert!(projection_b.rendered_prompt().contains("UNTRUSTED_ONLY"));
    assert_ne!(state_a.run_id, state_b.run_id);
    assert_eq!(
        fs::read_to_string(a.project.join("continued.txt"))
            .unwrap()
            .trim(),
        "WORKING"
    );
    assert_config(&home, &before);
    let report = json!({"observerKilled":killed,"gapVisibleInStatus":status[0]["gap"],"mainSessionContinued":true,"concurrentSeparateLedgers":true,"research":state_a,"untrusted":state_b,"globalConfigUnchanged":true,"passed":true});
    fs::write(
        evidence_directory().join("interruption-concurrency.json"),
        serde_json::to_string_pretty(&report).unwrap() + "\n",
    )
    .unwrap();
}

fn start(client: &mut Client, id: &str, text: &str) {
    client.request(
        "turn/start",
        json!({"threadId":id,"input":[{"type":"text","text":text}]}),
    );
}
fn finish(client: &mut Client, id: &str) {
    let completed = client.wait(Duration::from_secs(180), |v| {
        v["method"] == "turn/completed" && v["params"]["threadId"] == id
    });
    assert_eq!(completed["params"]["turn"]["status"], "completed");
}
fn state(id: &str) -> State {
    Store::read(&RunStore::default_state_root().unwrap(), id)
        .unwrap()
        .unwrap()
}

fn kill_observer(id: &str) -> u32 {
    let deadline = Instant::now() + Duration::from_secs(90);
    loop {
        if let Some(state) = Store::read(&RunStore::default_state_root().unwrap(), id).unwrap()
            && let Some(flight) = state.inflight
        {
            assert!(
                Command::new("/bin/kill")
                    .args(["-KILL", &flight.pid.to_string()])
                    .status()
                    .unwrap()
                    .success()
            );
            return flight.pid;
        }
        assert!(Instant::now() < deadline, "observer never became active");
        thread::sleep(Duration::from_millis(100));
    }
}

fn gap_status(id: &str) -> Value {
    let status = Command::new(env!("CARGO_BIN_EXE_kah"))
        .args(["status", id, "--json"])
        .output()
        .unwrap();
    assert!(status.status.success());
    let status: Value = serde_json::from_slice(&status.stdout).unwrap();
    assert_eq!(status[0]["gap"]["code"], "OBSERVER_INTERRUPTED", "{status}");
    status
}
