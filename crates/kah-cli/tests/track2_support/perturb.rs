use super::{
    fixture::{digest, save},
    journey::{Worker, idle},
    proof, telemetry,
};
use kah_cli::observer_review::{
    self as review,
    state::{Purpose, Store},
};
use serde_json::{Value, json};
use std::{fs, process::Command, time::Duration};

pub fn queue_backlog(worker: &Worker) {
    for n in 0..3 {
        review::queue::enqueue(&worker.id,Purpose::Checkpoint,&format!("track2-backlog-{n}"),
            json!({"events":[{"kind":"user_gate","source":"host_fixture","at_ms":kah_observer::session::now_ms()}]})).unwrap();
    }
    review::queue::enqueue(
        &worker.id,
        Purpose::Drift,
        "track2-queued-drift",
        json!({"source":"host_fixture","reason":"review worker drift alongside gate backlog"}),
    )
    .unwrap();
    save(
        &worker
            .root
            .join(format!("{}-backlog-start.json", worker.provider)),
        &json!({
        "at_ms":kah_observer::session::now_ms(),"entries":review::queue::entries(&worker.id).unwrap(),
        "results_before":telemetry::result_ids(&worker.id),"deferrals":Store::read(&worker.id).unwrap().unwrap().deferrals}),
    );
    review::queue::kick(&worker.id).unwrap();
    worker.pane.prompt("Could the marker requirement be RED instead? This might replace our BLUE constraint; I have not decided yet. Do not change any files; explain the implications.").unwrap();
    while !idle(&worker.pane) {
        std::thread::sleep(Duration::from_millis(200));
    }
}
pub fn during_review(worker: &Worker) {
    let initial = wait_pending(worker);
    let session = initial.observer.session.clone().unwrap();
    let pane = initial.observer.pane.clone().unwrap();
    let pid = initial.observer.pid.unwrap();
    let stamp = initial.observer.process_stamp.clone().unwrap();
    assert_eq!(
        kah_observer::session::process_stamp(pid).as_deref(),
        Some(stamp.as_str()),
        "TRACK2_PID_OWNER stage=restart"
    );
    let socket = initial.worker.socket.as_ref().unwrap();
    assert!(socket.to_string_lossy().contains("kah-gate-track2-"));
    // Native process restart in its own pane. No main session is addressed.
    assert!(
        Command::new("herdr")
            .args(["pane", "close", &pane])
            .status()
            .unwrap()
            .success()
    );
    let store = Store::open_wait(&worker.id).unwrap();
    let mut state = store.load().unwrap().unwrap();
    state.observer.pane = None;
    state.observer.pid = None;
    state.observer.process_stamp = None;
    state.stopped = true;
    store.save(&state).unwrap();
    drop(store);
    review::runtime::resume(&worker.id).unwrap();
    let after = wait_bound(worker);
    assert_eq!(
        after.observer.session.as_deref(),
        Some(session.as_str()),
        "TRACK2_OBSERVER_ID stage=resume"
    );
    save(
        &worker
            .root
            .join(format!("{}-observer-restart.json", worker.provider)),
        &json!({
        "request":initial.pending,"session_before":session,"session_after":after.observer.session,
        "pane_before":pane,"pane_after":after.observer.pane,"pid_before":pid,"pid_after":after.observer.pid}),
    );
    swap(worker, &after);
    hold_after_ack(worker);
}
fn swap(worker: &Worker, state: &review::state::State) {
    let replacement = std::path::PathBuf::from(
        std::env::var_os("KAH_TRACK2_SWAP_BINARY")
            .expect("TRACK2_SWAP_BINARY_REQUIRED stage=preflight"),
    );
    let private = state.observer.workspace.join("bin/kah");
    assert!(
        private.starts_with(&worker.home),
        "TRACK2_BINARY_SCOPE stage=swap"
    );
    let before = digest(&private);
    let after = digest(&replacement);
    assert_ne!(before, after, "TRACK2_SWAP_IDENTICAL stage=swap");
    let temporary = private.with_extension("track2-next");
    fs::copy(&replacement, &temporary).unwrap();
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(&temporary, fs::Permissions::from_mode(0o755)).unwrap();
    fs::rename(&temporary, &private).unwrap();
    save(
        &worker
            .root
            .join(format!("{}-binary-swap.json", worker.provider)),
        &json!({
        "request":state.pending,"path":private,"before_sha256":before,"after_sha256":digest(&private),
        "observer_session":state.observer.session,"mid_review":state.pending.is_some()}),
    );
}
fn hold_after_ack(worker: &Worker) {
    let state = wait_pending(worker);
    let request = state.pending.as_ref().unwrap().id.clone();
    let held = Store::open_wait(&worker.id).unwrap();
    let directory = held.directory.clone();
    let redelivery_before: Option<Value> =
        review::state::read(&directory.join("redelivery.json")).unwrap();
    loop {
        if state
            .observer
            .workspace
            .join("outbox")
            .join(format!("submitted-{request}.json"))
            .is_file()
            && native_ack(&state, &request)
        {
            break;
        }
        std::thread::sleep(Duration::from_millis(200));
    }
    let start = kah_observer::session::now_ms();
    // Exceed the existing 15s idle observation interval with real host processing.
    // This is a perturbation duration, never a review deadline.
    let processing = review::liveness::processing(&worker.id, &request).unwrap();
    for _ in 0..20 {
        std::thread::sleep(Duration::from_secs(1));
        review::liveness::run(&worker.id).unwrap();
    }
    let processing_end = kah_observer::session::now_ms();
    let redelivery_after: Option<Value> =
        review::state::read(&directory.join("redelivery.json")).unwrap();
    assert_eq!(
        redelivery_before, redelivery_after,
        "TRACK2_POST_ACK_REDELIVERY stage=consume"
    );
    let progress: Option<Value> =
        review::state::read(&directory.join("liveness/progress.json")).unwrap();
    assert!(
        progress.as_ref().is_some_and(|p| p["stalled"] == false),
        "TRACK2_POST_ACK_STALL stage=consume"
    );
    save(
        &worker
            .root
            .join(format!("{}-post-ack.json", worker.provider)),
        &json!({
        "request":request,"start_ms":start,"end_ms":processing_end,"progress":progress,
        "actual_native_ack":true,"redelivery_unchanged":true}),
    );
    let mut restarted = host_restart(worker, &request);
    drop(held);
    drop(processing);
    concurrent_writers(worker);
    assert!(
        restarted.wait().unwrap().success(),
        "TRACK2_HOST_RESTART stage=consume"
    );
    let after = Store::read(&worker.id).unwrap().unwrap();
    assert_eq!(
        after.observer.session, state.observer.session,
        "TRACK2_HOST_OBSERVER_ID stage=restart"
    );
}
fn host_restart(worker: &Worker, request: &str) -> std::process::Child {
    // Restart only a child created here, carrying the private HOME and worker ID.
    let mut host = Command::new(worker.home.join(".local/bin/kah"))
        .args(["__review-consume", &worker.id])
        .spawn()
        .unwrap();
    let first = host.id();
    host.kill().unwrap();
    host.wait().unwrap();
    let restarted = Command::new(worker.home.join(".local/bin/kah"))
        .args(["__review-consume", &worker.id])
        .spawn()
        .unwrap();
    let second = restarted.id();
    save(
        &worker
            .root
            .join(format!("{}-host-restart.json", worker.provider)),
        &json!({
        "request":request,"host_pid_before":first,"host_pid_after":second,"fixture_owned":true,
        "observer_session":Store::read(&worker.id).unwrap().unwrap().observer.session}),
    );
    restarted
}
fn native_ack(state: &review::state::State, request: &str) -> bool {
    let transcript = fs::read_to_string(state.observer.transcript.as_ref().unwrap()).unwrap();
    transcript
        .lines()
        .filter_map(|line| serde_json::from_str::<Value>(line).ok())
        .any(|row| {
            let content = if state.worker.provider == "codex"
                && row["payload"]["type"] == "function_call_output"
            {
                row["payload"]["output"].as_str().unwrap_or("").to_owned()
            } else if state.worker.provider == "claude" {
                row["message"]["content"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter(|p| p["type"] == "tool_result")
                    .map(|p| p["content"].to_string())
                    .collect::<Vec<_>>()
                    .join("\n")
            } else {
                String::new()
            };
            content.contains("evidence_submitted") && content.contains(request)
        })
}
fn concurrent_writers(worker: &Worker) {
    let mut children = vec![];
    for _ in 0..3 {
        children.push(
            Command::new(std::env::current_exe().unwrap())
                .args(["--exact", "fixture_writer", "--nocapture"])
                .env("KAH_TRACK2_WRITER", &worker.id)
                .spawn()
                .unwrap(),
        );
    }
    let statuses = children
        .into_iter()
        .map(|mut c| c.wait().unwrap().success())
        .collect::<Vec<_>>();
    save(
        &worker
            .root
            .join(format!("{}-concurrent.json", worker.provider)),
        &json!({"statuses":statuses,"store":Store::directory(&worker.id).unwrap()}),
    );
    assert!(
        statuses.iter().all(|s| *s),
        "TRACK2_CONCURRENT_WRITER stage=consume"
    );
}
pub fn write_run_store(worker: &str) {
    let base = kah_core::run_store::RunStore::default_state_root().unwrap();
    let binding = kah_observer::session::Store::read(&base, worker)
        .unwrap()
        .unwrap();
    assert!(
        base.starts_with(std::env::var_os("KAH_TRACK2_ROOT").unwrap()),
        "TRACK2_STORE_SCOPE stage=writer"
    );
    let run = binding.run_id.unwrap();
    for _ in 0..8 {
        let store = kah_core::run_store::RunStore::open_wait(&base, &binding.binding.project, &run)
            .unwrap();
        let recovered = store.recover().unwrap();
        store.commit_projection(&recovered.projection).unwrap();
    }
}
fn wait_pending(worker: &Worker) -> review::state::State {
    loop {
        if let Some(state) = Store::read(&worker.id).unwrap()
            && state.pending.is_some()
            && state.observer.session.is_some()
        {
            return state;
        }
        std::thread::sleep(Duration::from_millis(200));
    }
}
fn wait_bound(worker: &Worker) -> review::state::State {
    loop {
        if let Some(state) = Store::read(&worker.id).unwrap()
            && state.observer.pid.is_some()
        {
            return state;
        }
        std::thread::sleep(Duration::from_millis(200));
    }
}
pub fn finish(worker: &Worker, start: u64, commits: &[Value]) -> Value {
    loop {
        let state = Store::read(&worker.id).unwrap().unwrap();
        let queue = review::queue::entries(&worker.id).unwrap();
        if state.pending.is_none()
            && queue.is_empty()
            && !review::events::pending(&worker.id).unwrap()
        {
            break;
        }
        // Observation only: worker stays idle; product controllers must drain it.
        assert!(idle(&worker.pane), "TRACK2_IDLE_WORKER stage=followup");
        std::thread::sleep(Duration::from_millis(250));
    }
    let prior: Value = serde_json::from_slice(
        &fs::read(
            worker
                .root
                .join(format!("{}-backlog-start.json", worker.provider)),
        )
        .unwrap(),
    )
    .unwrap();
    let ids = telemetry::result_ids(&worker.id);
    let completed = ids
        .iter()
        .filter(|id| {
            !prior["results_before"]
                .as_array()
                .unwrap()
                .contains(&json!(id))
        })
        .count();
    let drain = proof::drain(
        prior["entries"].as_array().unwrap().len(),
        0,
        completed,
        prior["at_ms"].as_u64().unwrap(),
        kah_observer::session::now_ms(),
    )
    .unwrap();
    let scenarios = acceptance_receipts(worker, &ids);
    json!({"passed":drain["passed"]==true && scenarios["passed"]==true,"drain":drain,"journey_ms":kah_observer::session::now_ms()-start,
        "scenarios":scenarios,"commits":commits,"completed_ids":ids})
}
fn acceptance_receipts(worker: &Worker, ids: &[String]) -> Value {
    let mut checks = serde_json::Map::new();
    let mut receipts = serde_json::Map::new();
    for name in [
        "observer-restart",
        "binary-swap",
        "post-ack",
        "host-restart",
        "concurrent",
    ] {
        let path = worker.root.join(format!("{}-{name}.json", worker.provider));
        checks.insert(name.into(), json!({"path":path,"sha256":digest(&path)}));
        receipts.insert(
            name.into(),
            serde_json::from_slice(&fs::read(&path).unwrap()).unwrap(),
        );
    }
    let scenarios = super::scenarios::verify(worker, ids);
    let validated = proof::perturbations(&json!(receipts))
        .unwrap_or_else(|e| json!({"passed":false,"reason":e}));
    json!({"passed":scenarios["passed"]==true && validated["passed"]==true,"receipts":checks,"checks":scenarios,"perturbations":validated})
}
