//! Frozen real light snapshot through a full native TUI, never a seeded task.
//! Native --no-alt-screen: openai/codex rust-v0.160.0 codex-rs/tui/src/cli.rs;
//! launch/receipt/cleanup use the existing private Herdr fixture mechanisms.
use super::*;
use kah_cli::observer_review::{
    packet_references, snapshot, source_store,
    state::{Request, Store as Reviews},
};
#[derive(serde::Deserialize)]
struct ReplayCase {
    request: String,
    worker: String,
    native_tokens_before: u64,
}
const START: &str = "steer: Remain idle as a private transport for a controller review of archived work. Do not create files, implement a task, commit, tag, push or dispatch workers. Acknowledge readiness; the archived snapshot, not this empty transport project, is the subject.";

pub fn run(root: &Path) -> Value {
    let case: ReplayCase =
        serde_json::from_slice(&fs::read(root.join("retained-light-case.json")).unwrap()).unwrap();
    assert!(
        std::env::var("HERDR_SOCKET_PATH")
            .unwrap()
            .contains("kah-gate-")
    );
    let home = PathBuf::from(std::env::var_os("HOME").unwrap());
    let before = crate::native_controller_support::global_settings(&home);
    let (launch, mut fixture) = crate::fixture::prepare(&home, "research", false);
    let mut command = launch.command(false);
    command
        .args(["--no-alt-screen", START])
        .env_remove("CODEX_THREAD_ID");
    let mut panes = Panes(vec![Pane::start(&command).unwrap()]);
    let worker = ready(&panes.0[0], "codex");
    fixture.sessions.push(worker.clone());
    let cleanup = super::review::Cleanup {
        home: home.clone(),
        worker: worker.clone(),
    };
    // An idle transport with no tools has no tripwire trigger yet. Initialize
    // the normal binding, without creating an observer turn or synthetic work.
    let store = Reviews::open_wait(&worker).unwrap();
    if store.load().unwrap().is_none() {
        let state = kah_cli::observer_review::state::State::new(
            kah_cli::relay_store::endpoint(&worker).unwrap(),
            &home,
        )
        .unwrap();
        store.save(&state).unwrap();
    }
    drop(store);
    let state = super::review::quiescent(&worker, "codex");
    let (original, packet_bytes) = scoped(&worker, state, &case);
    let id = original.id.clone();
    let submitted = retained_submission(&original);
    let pane = Pane {
        id: submitted["pane"].as_str().unwrap().into(),
    };
    if !panes.0.iter().any(|p| p.id == pane.id) {
        panes.0.push(pane.clone());
    }
    let reviewed = super::review::wait_review(&worker, &pane, "codex");
    assert_eq!(reviewed.latest.as_ref().unwrap()["request"], id);
    finish(root, &home, &reviewed, packet_bytes, &before, &case);
    let result =
        serde_json::from_slice(&fs::read(root.join("retained-light.json")).unwrap()).unwrap();
    drop(panes);
    drop(cleanup);
    result
}
fn scoped(
    worker: &str,
    mut state: kah_cli::observer_review::state::State,
    case: &ReplayCase,
) -> (Request, (usize, usize)) {
    let mut original: Request = source_store::read(
        &Reviews::directory(&case.worker)
            .unwrap()
            .join("requests")
            .join(format!("{}.json", case.request)),
    )
    .unwrap();
    assert_eq!(original.id, case.request);
    assert_eq!(original.trigger["review_class"], "commit");
    let original_intent = original.intent.clone();
    let bytes_before = bytes(&original);
    original.worker = worker.into();
    original.id = format!(
        "review-{}-{}",
        kah_observer::session::now_ms(),
        std::process::id()
    );
    original.trigger["historical_replay"] = json!({"original_request":case.request,"subject_worker":case.worker,"original_sha":original.head,"original_created_ms":original.created_ms});
    original.trigger["subject"] = json!(
        "Review the archived worker and exact captured SHA/evidence. The disposable transport project is not the subject; do not infer its empty tree is the delivered code."
    );
    original.created_ms = kah_observer::session::now_ms();
    let store = Reviews::open_wait(worker).unwrap();
    state.deferrals = original.intent["open_deferrals"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    store.save(&state).unwrap();
    source_store::initialize(&store.directory, &state.observer.workspace).unwrap();
    packet_references::scope(&store.directory, &state.observer.workspace, &mut original).unwrap();
    assert_eq!(original.intent, original_intent);
    let bytes_after = bytes(&original);
    drop(store);
    (original, (bytes_before, bytes_after))
}
fn bytes(request: &Request) -> usize {
    request.documents.iter().map(|d| d.text.len()).sum()
}
fn finish(
    root: &Path,
    home: &Path,
    state: &kah_cli::observer_review::state::State,
    packet_bytes: (usize, usize),
    configs: &[(PathBuf, Vec<u8>)],
    case: &ReplayCase,
) {
    let worker = &state.worker.session;
    let id = state.latest.as_ref().unwrap()["request"].as_str().unwrap();
    let (before, after) = packet_bytes;
    let result: Value = source_store::read(
        &Reviews::directory(worker)
            .unwrap()
            .join("results")
            .join(format!("{id}.json")),
    )
    .unwrap();
    assert_eq!(result["judgment"]["backend"], "jev");
    assert!(result["native_tokens"].as_u64().is_some_and(|n| n > 0));
    super::review_trace::archive(home, state).unwrap();
    for (path, bytes) in configs {
        assert_eq!(fs::read(path).unwrap(), *bytes);
    }
    let output = json!({"passed":true,"versions":versions(home),"model":"gpt-6.1-sol","effort":"low","original_request":case.request,"subject_worker":case.worker,
        "replay_request":id,"sha":result["head"],"goal_and_items_unchanged":true,"packet_bytes_before":before,"packet_bytes_after":after,
        "native_tokens_before":case.native_tokens_before,"native_tokens_after":result["native_tokens"],"native_history_and_cache_differ":true,"seeded_completed_task":false,
        "assessment":result["assessment"]["state"],"elapsed_ms":result["elapsed_ms"],"observer":state.observer.session,"pane":state.observer.pane,
        "mechanics_only":true,"phase5o_gate_passed":false,"global_configs_unchanged":true,"private_herdr":true});
    fs::write(
        root.join("retained-light.json"),
        serde_json::to_vec_pretty(&output).unwrap(),
    )
    .unwrap();
}

fn retained_submission(request: &Request) -> Value {
    match snapshot::wake(request) {
        Ok(value) => value,
        Err(error)
            if error
                .to_string()
                .starts_with("REVIEW_SUBMISSION_UNCONFIRMED") =>
        {
            // Native startup may exceed the short delivery handshake; the
            // product retains this exact request and starts its liveness owner
            // before delivery (runtime::submit_snapshot). Await that recovery.
            let state = Reviews::read(&request.worker).unwrap().unwrap();
            assert_eq!(state.pending.as_ref().unwrap().id, request.id);
            eprintln!("Retained cold handshake: {error}");
            json!({"status":"delivery_retained","request":request.id,"pane":state.observer.pane.unwrap()})
        }
        Err(error) => panic!("Snapshot failed outside retained delivery: {error}"),
    }
}
