//! Real idle native observer, undelivered publication and held controller lock.
use super::*;
use kah_cli::observer_review::{
    capture, liveness,
    state::{Pending, Purpose, Store as Reviews},
    workspace,
};

pub fn run(worker: &str, provider: &str, panes: &mut Panes) -> Value {
    let home = PathBuf::from(std::env::var_os("HOME").unwrap());
    let (launch, mut courier) = crate::fixture::prepare(&home, "research", false);
    let mut command = launch.command(false);
    command.args(["--no-alt-screen", "You are a disposable notice courier. On kah observer stall notices reply NOTICE_RECEIVED. Do not use tools or answer questions."]).env_remove("CODEX_THREAD_ID");
    let receiver = Pane::start(&command).unwrap();
    courier.sessions.push(ready(&receiver, "codex"));
    Command::new("herdr")
        .args(["agent", "rename", &receiver.id, "kah-reviewer"])
        .status()
        .unwrap();
    panes.0.push(receiver.clone());
    let baseline = Reviews::read(worker).unwrap().unwrap();
    let mut outcomes = vec![];
    for fault in [
        "idle pending request",
        "held review lock",
        "crash after publish before delivery",
    ] {
        outcomes.push(fault_case(worker, provider, &receiver, &baseline, fault));
    }
    receiver.close().unwrap();
    json!({"cases":outcomes,"sameObserverSession":baseline.observer.session,"sameObserverPane":baseline.observer.pane,"passed":true})
}

fn fault_case(
    worker: &str,
    provider: &str,
    receiver: &Pane,
    baseline: &kah_cli::observer_review::state::State,
    fault: &str,
) -> Value {
    let mut state = Reviews::read(worker).unwrap().unwrap();
    assert!(state.pending.is_none());
    let request = capture::request(
        &state,
        Purpose::Drift,
        fault,
        json!({"fixture":"F20 delivery fault; inspect actual worker intent","fault":fault}),
    )
    .unwrap();
    let store = Reviews::open(worker).unwrap();
    workspace::publish(&store, &state, &request).unwrap();
    state.pending = Some(Pending::from(&request));
    store.save(&state).unwrap();
    let mut held = (fault == "held review lock").then_some(store);
    // Native observer remains idle: simulate a crash before the delivery call.
    observe(worker);
    std::thread::sleep(Duration::from_secs(16));
    observe(worker);
    let root = Reviews::directory(worker).unwrap().join("liveness");
    let notice = liveness::notice(worker).unwrap().unwrap();
    assert!(
        notice["systemMessage"]
            .as_str()
            .unwrap()
            .contains(&request.id)
    );
    let delivered: Value =
        serde_json::from_slice(&fs::read(root.join("reviewer.json")).unwrap()).unwrap();
    assert_eq!(delivered["submitted"], true);
    assert_eq!(delivered["request"], request.id);
    let pane = Pane {
        id: baseline.observer.pane.clone().unwrap(),
    };
    // A blocked detached retry proceeds when the held lock is released.
    // Other cases use the same event-driven redelivery path.
    drop(held.take());
    let recovered = automatic_recovery(worker, &pane, provider);
    assert_eq!(recovered.latest.as_ref().unwrap()["request"], request.id);
    assert_eq!(recovered.observer.session, baseline.observer.session);
    assert_eq!(recovered.observer.pane, baseline.observer.pane);
    observe(worker);
    evidence(
        &root,
        &request.id,
        fault,
        receiver,
        notice,
        delivered,
        recovered.latest,
    )
}

fn automatic_recovery(
    worker: &str,
    pane: &Pane,
    provider: &str,
) -> kah_cli::observer_review::state::State {
    loop {
        let state = Reviews::read(worker).unwrap().unwrap();
        if state.pending.is_none() {
            return state;
        }
        // Read the native pane to answer terminal queries. Do not manually
        // re-prompt: this journey must prove the detached product retry.
        pane.logs(provider, None, None).unwrap();
        let path = Reviews::directory(worker).unwrap().join("gap.json");
        if let Ok(bytes) = fs::read(path) {
            let gap: Value = serde_json::from_slice(&bytes).unwrap();
            assert!(
                kah_cli::observer_review::redelivery::retryable(gap["error"].as_str().unwrap()),
                "{gap}"
            );
        }
        std::thread::sleep(Duration::from_millis(250));
    }
}

fn observe(worker: &str) {
    let binary = PathBuf::from(std::env::var_os("HOME").unwrap()).join(".local/bin/kah");
    let result = Command::new(binary)
        .args(["__review-liveness", worker])
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
}

fn evidence(
    root: &Path,
    request: &str,
    fault: &str,
    receiver: &Pane,
    notice: Value,
    delivered: Value,
    judgment: Option<Value>,
) -> Value {
    let events = fs::read_to_string(root.join("events.jsonl")).unwrap();
    let rows = events
        .lines()
        .map(|s| serde_json::from_str::<Value>(s).unwrap())
        .filter(|v| v["progress"]["sample"]["request"] == request)
        .collect::<Vec<_>>();
    assert!(rows.iter().any(|r| r["state"] == "stalled"));
    assert!(rows.iter().any(|r| r["state"] == "recovered"));
    assert!(
        receiver
            .logs("codex", None, None)
            .unwrap()
            .contains(request)
    );
    json!({"fault":fault,"request":request,"hookNotice":notice,"reviewerNotice":delivered,"events":rows,"judgment":judgment,"passed":true})
}
