//! A real explicit gate outranks routine rechecks of actual native user input.
use super::*;
use kah_cli::observer_review::{
    control, events, queue, runtime,
    state::{Purpose, Request, Store as Reviews},
};
const TASK: &str = "Create format.cjs exporting format, which parses JSON then returns compact JSON with top-level keys sorted lexicographically. Run a Node demonstration. Do not commit or claim a phase complete; await my next direction.";
const ASK: &str = "Should the JSON formatter preserve original whitespace instead? I am considering changing the policy, but have not decided. Explain the implications without changing the formatter or claiming a phase complete.";

pub fn run(root: &Path) -> Value {
    assert!(
        std::env::var("HERDR_SOCKET_PATH")
            .unwrap()
            .contains("kah-gate-")
    );
    let registration = serde_json::from_str(include_str!(
        "../../../../docs/gates/evidence/phase-5o-c/v6-cases-frozen.json"
    ))
    .unwrap();
    super::checkpoint_v6_heldout::verify(&registration);
    let home = PathBuf::from(std::env::var_os("HOME").unwrap());
    let configs = [
        home.join(".codex/config.toml"),
        home.join(".claude/settings.json"),
    ]
    .map(|p| (p.clone(), fs::read(p).unwrap()));
    let (launch, mut fixture) = crate::fixture::prepare(&home, "research", false);
    let mut command = launch.command(false);
    command
        .args(["--no-alt-screen", TASK])
        .env_remove("CODEX_THREAD_ID");
    let mut panes = Panes(vec![Pane::start(&command).unwrap()]);
    let worker = ready(&panes.0[0], "codex");
    fixture.sessions.push(worker.clone());
    let cleanup = super::review::Cleanup {
        home: home.clone(),
        worker: worker.clone(),
    };
    super::linked_steer::steer(
        &home,
        &worker,
        ASK,
        &format!("queue-ask-{}", kah_observer::session::now_ms()),
        None,
    );
    super::linked_steer::wait_input(&worker, ASK);
    ready(&panes.0[0], "codex");
    let source = Store::open(
        &RunStore::default_state_root().unwrap(),
        &worker,
        Duration::from_secs(30),
    )
    .unwrap()
    .sources()
    .unwrap()
    .into_iter()
    .find(|i| i.text == ASK)
    .unwrap();
    let result = journey(&home, &worker, &source, &mut panes);
    for (path, bytes) in configs {
        assert_eq!(fs::read(path).unwrap(), bytes);
    }
    drop(panes);
    drop(cleanup);
    json!({"passed":true,"versions":versions(&home),"models":{"codex":"gpt-6.1-sol/low"},"privateHerdr":true,"globalConfigsUnchanged":true,"queueOrder":result,"root":root})
}

fn journey(
    home: &Path,
    worker: &str,
    source: &kah_observer::intake::Input,
    panes: &mut Panes,
) -> Value {
    active_review(worker, panes);
    let store = Reviews::open_wait(worker).unwrap();
    let state = store.load().unwrap().unwrap();
    let pending = state
        .pending
        .clone()
        .expect("a real native review must be active");
    assert_ne!(pending.purpose, Purpose::Checkpoint);
    // This short lock protects the queue observation, not the observer TUI.
    // Native evidence gathering continues while two real input rechecks and
    // the actual CLI checkpoint are durably enqueued.
    for turn in ["routine-json-one", "routine-json-two"] {
        queue::enqueue(worker, Purpose::Steering, turn, json!({"uncertain_user_input":source,"reason":"routine recheck of actual tentative JSON policy input"})).unwrap();
    }
    let output = Command::new(home.join(".local/bin/kah"))
        .args(["gate", worker])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    events::transfer(worker).unwrap();
    let status = control::status(worker).unwrap();
    let jobs = queue::entries(worker).unwrap();
    assert_eq!(jobs[0].1.purpose, Purpose::Checkpoint);
    for turn in ["routine-json-one", "routine-json-two"] {
        assert!(jobs.iter().any(|(_, j)| j.turn == turn));
    }
    super::ambiguity::retain(
        home,
        "queue-before-release",
        &json!({"worker":worker,"pending":pending,"status":status,"jobs":jobs.iter().map(|(_,j)|j).collect::<Vec<_>>(),"nativeSource":source}),
    );
    let existing = fs::read_dir(store.directory.join("requests"))
        .unwrap()
        .map(|e| e.unwrap().path())
        .collect::<Vec<_>>();
    drop(store);
    let next = next_request(worker, &pending.id, &existing, panes);
    let checkpoint_result = super::ambiguity::wait_result(worker, &next.id, panes);
    let first_result = super::ambiguity::read_result(worker, &pending.id);
    let actual = Reviews::read(worker).unwrap().unwrap();
    let same_session = first_result["observer_session"] == checkpoint_result["observer_session"]
        && first_result["observer_session"].is_string();
    let result = json!({"passed":next.purpose==Purpose::Checkpoint&&same_session&&preserved(worker),"worker":worker,"firstPending":pending,"firstResult":first_result,"nextRequest":next,"checkpointResult":checkpoint_result,"statusBeforeRelease":status,"sameObserverSession":same_session,"sameObserverPane":actual.observer.pane==state.observer.pane,"queuedRoutineTriggersPreserved":preserved(worker),"protocol":"Real CLI kah gate, real tentative relayed input, native review in progress; checkpoint review priority is independent of its alignment verdict."});
    super::ambiguity::retain(home, "queue-next-native-review", &result);
    assert_eq!(next.purpose, Purpose::Checkpoint);
    assert!(same_session);
    assert!(preserved(worker));
    json!({"passed":true,"worker":worker,"nextPurpose":next.purpose,"checkpointRequest":next.id,"checkpointAssessment":checkpoint_result["assessment"],"observerSession":actual.observer.session,"sameObserverSession":true,"sameObserverPane":true,"queuedRoutineTriggersPreserved":true})
}

fn active_review(worker: &str, panes: &mut Panes) {
    loop {
        let state = Reviews::read(worker).unwrap();
        if let Some(state) = state
            && let Some(pending) = state.pending
        {
            if pending.purpose != Purpose::Checkpoint {
                return;
            }
            let pane = Pane {
                id: state.observer.pane.unwrap(),
            };
            if !panes.0.iter().any(|p| p.id == pane.id) {
                panes.0.push(pane.clone());
            }
            super::review::wait_review(worker, &pane, "codex");
        } else {
            runtime::wake(worker, Purpose::Drift, "queue-inflight", json!({"event":"Review actual JSON formatter progress and its tentative user policy question."})).unwrap();
            while Reviews::read(worker)
                .unwrap()
                .is_none_or(|s| s.pending.is_none())
            {
                // A queued wake waiting on another host job's lock must not
                // generate another trigger on every read of the same state.
                queue::drain(worker).unwrap();
                std::thread::sleep(Duration::from_millis(200));
            }
        }
    }
}

fn next_request(worker: &str, first: &str, existing: &[PathBuf], panes: &mut Panes) -> Request {
    let root = Reviews::directory(worker).unwrap();
    let mut retried = None;
    loop {
        queue::drain(worker).unwrap();
        for entry in fs::read_dir(root.join("requests")).unwrap() {
            let path = entry.unwrap().path();
            if existing.contains(&path) || path.extension().is_none_or(|e| e != "json") {
                continue;
            }
            let request: Request = kah_cli::observer_review::state::read(&path)
                .unwrap()
                .unwrap();
            if request.id != first {
                return request;
            }
        }
        let state = Reviews::read(worker).unwrap().unwrap();
        if let Some(id) = state.observer.pane {
            let pane = Pane { id };
            if !panes.0.iter().any(|p| p.id == pane.id) {
                panes.0.push(pane.clone());
            }
            let screen = pane.logs("codex", None, None).unwrap();
            if screen.contains("Open restricted") {
                crate::native_controller_support::native_consent(&pane.id, "codex");
            }
            super::checkpoint_retry::gap(worker, &mut retried);
        }
        std::thread::sleep(Duration::from_millis(200));
    }
}
fn preserved(worker: &str) -> bool {
    let root = Reviews::directory(worker).unwrap();
    ["routine-json-one", "routine-json-two"].iter().all(|turn| {
        ["queued", "covered-triggers"].iter().any(|directory| {
            fs::read_dir(root.join(directory))
                .into_iter()
                .flatten()
                .any(|entry| {
                    let path = entry.unwrap().path();
                    path.extension().is_some_and(|e| e == "json")
                        && kah_cli::observer_review::state::read::<queue::Wake>(&path)
                            .unwrap()
                            .is_some_and(|wake| wake.turn == *turn)
                })
        })
    })
}
