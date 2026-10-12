//! Real VF-GOOD event control copy; restart is a host process, no agents/Jev.
use kah_cli::observer_review::{
    option1, queue, source_store,
    state::{State, Store},
};
use serde_json::{Value, json};
use std::{fs, path::Path, process::Command};

const REPLAY: &str =
    include_str!("../../../docs/gates/evidence/phase3-checkpoint-carry/durable-wait-input.json");

#[test]
fn retained_same_sha_events_wait_across_host_handoff_and_restart_then_carry_once() {
    std::env::var_os("XDG_STATE_HOME").expect("run through demos/check-tests.sh");
    if let Ok(worker) = std::env::var("KAH_GATE_WAIT_CHILD_WORKER") {
        assert!(queue::claim_batch(&worker).unwrap().is_none());
        return;
    }
    let input: Value = serde_json::from_str(REPLAY).unwrap();
    let worker = format!("durable-wait-{}", kah_cli::relay_store::nonce());
    let directory = prepare(&worker, &input);
    assert!(
        queue::claim_batch(&worker).unwrap().is_none(),
        "tag/end must wait after ACK releases native slot"
    );
    restart(&worker);
    source_store::save(
        &directory
            .join("gate-host")
            .join(input["gate"]["id"].as_str().unwrap()),
        "stage.json",
        &json!({"stage":"failed","reason":"retained GIT_CONTROL_BOUNDARY"}),
    )
    .unwrap();
    restart(&worker); // A retained host failure cannot turn into a new review.
    unrelated_commit_still_runs(&worker);
    finish_gate(&worker, &input);
    drain_carried(&worker, &input);
    assert!(queue::claim_batch(&worker).unwrap().is_none());
    let saved = Store::read(&worker).unwrap().unwrap();
    assert_eq!(saved.wakes, 0);
    assert!(saved.pending.is_none());
    assert_eq!(
        saved.checkpoint.as_ref().unwrap()["request"],
        input["gate"]["id"]
    );
    assert_eq!(
        saved.checkpoint.unwrap()["completion_key"],
        input["wakes"][1]["wake"]["trigger"]["events"][0]["completion_key"]
    );
    fs::remove_dir_all(directory.parent().unwrap()).unwrap();
}
fn prepare(worker: &str, input: &Value) -> std::path::PathBuf {
    let store = Store::open(worker).unwrap();
    let directory = store.directory.clone();
    let state = initial(input, worker, &directory);
    store.save(&state).unwrap();
    kah_core::run_store::ensure_private_directory(&directory.join("covered-triggers")).unwrap();
    kah_core::run_store::ensure_private_directory(
        &directory
            .join("gate-host")
            .join(input["gate"]["id"].as_str().unwrap()),
    )
    .unwrap();
    let mut gate = input["gate"].clone();
    gate["worker"] = worker.into();
    source_store::save(
        &directory
            .join("gate-host")
            .join(input["gate"]["id"].as_str().unwrap()),
        "job.json",
        &json!({"request":gate}),
    )
    .unwrap();
    for row in input["wakes"].as_array().unwrap() {
        let wake = serde_json::from_value(row["wake"].clone()).unwrap();
        queue::enqueue_once_at(&directory, row["name"].as_str().unwrap(), &wake).unwrap();
    }
    drop(store);
    directory
}
fn finish_gate(worker: &str, input: &Value) {
    let store = Store::open(worker).unwrap();
    let directory = store.directory.clone();
    let mut saved = store.load().unwrap().unwrap();
    saved.checkpoint = Some(
        json!({"head":input["head"],"intent_revision":input["revision"],
        "request":input["gate"]["id"],"state":"checkpoint_needs_thinker","passed":null}),
    );
    store.save(&saved).unwrap();
    source_store::save(
        &directory
            .join("gate-host")
            .join(input["gate"]["id"].as_str().unwrap()),
        "finished.json",
        &json!({"assessment":{"passed":null}}),
    )
    .unwrap();
    drop(store);
    restart(worker); // Finished evaluation still waits for verify-first's conclusion.
    let store = Store::open(worker).unwrap();
    let mut saved = store.load().unwrap().unwrap();
    let checkpoint = saved.checkpoint.as_mut().unwrap();
    checkpoint["state"] = "checkpoint_unconfirmed".into();
    checkpoint["terminal"] = true.into();
    store.save(&saved).unwrap();
}
fn initial(input: &Value, worker: &str, root: &Path) -> State {
    serde_json::from_value(json!({"worker":{"provider":"claude","session":worker,"project":root,"pane":null,"socket":null},
        "observer":{"session":"recorded-observer","pane":null,"name":"recorded-observer","workspace":root,
            "model":"claude-sonnet-5-5","effort":"low","transcript":null},
        "pending":null,"latest":null,"checkpoint":input["original_gate_state"],"last_verified_turn":null,
        "deferrals":[],"wakes":0,"native_tokens":0,"stopped":false})).unwrap()
}
fn restart(worker: &str) {
    assert!(
        Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "retained_same_sha_events_wait_across_host_handoff_and_restart_then_carry_once",
                "--nocapture"
            ])
            .env("KAH_GATE_WAIT_CHILD_WORKER", worker)
            .status()
            .unwrap()
            .success()
    );
}
fn unrelated_commit_still_runs(worker: &str) {
    queue::enqueue(
        worker,
        kah_cli::observer_review::state::Purpose::Checkpoint,
        "independent-commit",
        json!({"review_class":"commit","head":"9".repeat(40)}),
    )
    .unwrap();
    let (wake, paths) = queue::claim_batch(worker).unwrap().unwrap();
    assert_eq!(wake.trigger["review_class"], "commit");
    queue::archive_submitted(&Store::directory(worker).unwrap(), &paths).unwrap();
    assert!(queue::claim_batch(worker).unwrap().is_none());
}
fn drain_carried(worker: &str, input: &Value) {
    for expected in input["wakes"].as_array().unwrap() {
        let (wake, paths) = queue::claim_batch(worker).unwrap().unwrap();
        assert_eq!(
            serde_json::to_value(&wake).unwrap()["trigger"]["events"],
            expected["wake"]["trigger"]["events"]
        );
        let store = Store::open(worker).unwrap();
        let mut state = store.load().unwrap().unwrap();
        let result = option1::carry::admit(
            &store,
            &mut state,
            input["head"].as_str().unwrap(),
            input["revision"].as_u64().unwrap(),
            &wake.turn,
            &wake.trigger,
        )
        .unwrap()
        .unwrap();
        assert_eq!(result["carried_from"], input["gate"]["id"]);
        assert!(result["passed"].is_null());
        assert_eq!(result["new_native_decisions"], 0);
        assert_eq!(result["new_jev_decisions"], 0);
        drop(store);
        queue::archive_submitted(&Store::directory(worker).unwrap(), &paths).unwrap();
    }
}
