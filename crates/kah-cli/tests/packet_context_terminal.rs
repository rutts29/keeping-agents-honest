//! Drive ordinary consume::run with a durably known terminal, without a judge
//! call or native session. The live builder2 request is the separate journey.
use kah_cli::{
    observer_review::{
        consume,
        preparation::{self, batch},
        source_store,
        state::{Pending, Purpose, Request, State, Store},
    },
    relay_store::Endpoint,
};
use kah_observer::{
    context_selection::{Manifest, Selection},
    session,
};
use serde_json::{Value, json};
use std::{fs, path::PathBuf, time::Duration};
fn setup(worker: &str) -> (Store, State) {
    let base = kah_core::run_store::RunStore::default_state_root().unwrap();
    let directory = Store::directory(worker).unwrap();
    let binding = session::Binding {
        session: worker.into(),
        project: directory.clone(),
        codex_home: directory.clone(),
        native: PathBuf::from("/nonexistent/native"),
        model: "gpt-6.1-sol".into(),
        effort: "low".into(),
        timeout_secs: 90,
        judge: Default::default(),
        jev_model: "jev-latest".into(),
    };
    let mut primary = session::Store::open(&base, worker, Duration::ZERO).unwrap();
    primary.save(&session::State::new(binding)).unwrap();
    drop(primary);
    let store = Store::open(worker).unwrap();
    let state = State::new(
        Endpoint {
            provider: "codex".into(),
            session: worker.into(),
            project: store.directory.clone(),
            pane: None,
            socket: None,
        },
        &store.directory,
    )
    .unwrap();
    fs::create_dir_all(state.observer.workspace.join("outbox")).unwrap();
    (store, state)
}
fn packet(turn: &str) -> Value {
    json!({"source":format!("{turn}.json"),"turn":turn,"trigger":{"preparation":{"purpose":"drift","state":{"intent":{"goal":"Whole authority."},"event":{"actions":[]}},"questions":{},"reason":"retained provider rejection"}}})
}
#[test]
fn ordinary_single_preparation_releases_the_same_unconfirmed_terminal() {
    let worker = format!("single-context-terminal-{}", kah_cli::relay_store::nonce());
    let store = staged_single(&worker);
    let root = store.directory.parent().unwrap().to_owned();
    drop(store);
    consume::run(&worker).unwrap();
    let state = Store::read(&worker).unwrap().unwrap();
    assert!(state.pending.is_none());
    assert!(state.latest.as_ref().unwrap()["assessment"]["passed"].is_null());
    assert!(
        Store::directory(&worker)
            .unwrap()
            .join("context-observation-review-single-terminal.json")
            .exists()
    );
    fs::remove_dir_all(root).unwrap();
}
fn manifest(job: &preparation::Job) -> Manifest {
    Manifest {
        decision: job.decision.clone(),
        selected: job
            .items
            .iter()
            .map(|i| Selection {
                id: i.id.clone(),
                digest: i.digest.clone(),
                reason: "whole original retained".into(),
                superseded_by: None,
            })
            .collect(),
        excluded: vec![],
    }
}
#[test]
fn ordinary_consume_retains_terminal_packet_and_finishes_the_other_packet() {
    let worker = format!("packet-terminal-{}", kah_cli::relay_store::nonce());
    let store = staged(&worker);
    let root = store.directory.parent().unwrap().to_owned();
    drop(store);
    consume::run(&worker).unwrap();
    let saved = Store::read(&worker).unwrap().unwrap();
    assert!(saved.pending.is_none());
    let results = saved.latest.as_ref().unwrap()["results"]
        .as_array()
        .unwrap();
    assert_eq!(results.len(), 2);
    assert_eq!(results[0]["terminal"], "unconfirmed_context");
    assert!(results[0]["passed"].is_null());
    assert_eq!(results[1]["previous_ready_receipt"], true);
    assert!(!Store::directory(&worker).unwrap().join("gap.json").exists());
    assert!(
        Store::directory(&worker)
            .unwrap()
            .join("reminders")
            .is_dir()
    );
    next_batch(&worker);
    fs::remove_dir_all(root).unwrap();
}
fn next_batch(worker: &str) {
    use kah_cli::observer_review::queue;
    queue::enqueue(
        worker,
        Purpose::Preparation,
        "first",
        packet("first")["trigger"].clone(),
    )
    .unwrap();
    assert!(
        queue::claim_batch(worker).unwrap().is_none(),
        "same terminal source must not be admitted under a new review id"
    );
    assert!(queue::header_entries(worker).unwrap().is_empty());
    assert!(
        Store::directory(worker)
            .unwrap()
            .join("terminal-packet-links")
            .is_dir()
    );
    let mut changed = packet("first")["trigger"].clone();
    changed["preparation"]["questions"] =
        serde_json::to_value(kah_observer::verified_judgment::questions()).unwrap();
    queue::enqueue(worker, Purpose::Preparation, "first", changed).unwrap();
    assert!(
        queue::claim_batch(worker).unwrap().is_some(),
        "changed questions are eligible again"
    );
    let mut changed_source = packet("first");
    changed_source["trigger"]["preparation"]["state"]["intent"]["goal"] =
        json!("Changed whole authority.");
    assert!(
        batch::terminal_packets::lookup(worker, &changed_source)
            .unwrap()
            .is_none(),
        "changed original source is eligible again"
    );
}

fn staged(worker: &str) -> Store {
    let (store, mut state) = setup(worker);
    let packets = vec![packet("first"), packet("second")];
    let request = Request {
        id: "review-packet-terminal".into(),
        worker: worker.into(),
        purpose: Purpose::Preparation,
        turn: "first".into(),
        head: None,
        intent: json!({"goal":"Whole authority."}),
        trigger: json!({"preparation_packets":packets}),
        documents: vec![],
        created_ms: 1,
    };
    state.pending = Some(Pending::from(&request));
    store.save(&state).unwrap();
    kah_cli::unattended_report::save(&store.directory,"gap.json",
        &json!({"request":request.id,"error":"PREPARATION_CONTEXT_UNCONFIRMED stage=context_preparation"})).unwrap();
    let requests = store.directory.join("requests");
    fs::create_dir_all(&requests).unwrap();
    source_store::save(
        &requests,
        &format!("{}.json", request.id),
        &serde_json::to_value(&request).unwrap(),
    )
    .unwrap();
    batch::packets::stage_packets(&request).unwrap();
    let active = preparation::active_decision(worker, &request.id).unwrap();
    let batch = batch::load(worker, &active).unwrap().unwrap();
    let selections = seed(&store, &request, &packets);
    kah_cli::unattended_report::save(
        &state.observer.workspace.join("outbox"),
        &format!("selected-{}.json", batch.id),
        &json!({"batch":batch.id,"selections":selections}),
    )
    .unwrap();
    store
}

fn seed(store: &Store, request: &Request, packets: &[Value]) -> Vec<Manifest> {
    let mut selections = vec![];
    for (index, packet) in packets.iter().enumerate() {
        let job = preparation::batch::packets::job_for_packet(request, index, packet).unwrap();
        let manifest = manifest(&job);
        selections.push(manifest.clone());
        if index == 0 {
            let mut local = request.clone();
            local.id = job.decision.clone();
            preparation::rejection::record(
                &local,
                &job,
                &manifest,
                &job.state,
                "JEV_CONTEXT_LIMIT stage=judge_http http_status=400",
            )
            .unwrap();
            preparation::rejection::record(
                &local,
                &job,
                &manifest,
                &job.state,
                "JEV_CONTEXT_LIMIT stage=judge_http http_status=400",
            )
            .unwrap();
        } else {
            let identity = kah_observer::context_selection::digest(
                &json!({"original":job.state,"questions":job.questions,"packet":packet}),
            );
            source_store::save(
                &store.directory,
                &format!("packet-result-{}.json", job.decision),
                &json!({"original_digest":identity,"previous_ready_receipt":true}),
            )
            .unwrap();
        }
    }
    selections
}

fn staged_single(worker: &str) -> Store {
    let (store, mut state) = setup(worker);
    let packet = packet("original");
    let request = Request {
        id: "review-single-terminal".into(),
        worker: worker.into(),
        purpose: Purpose::Preparation,
        turn: "original".into(),
        head: None,
        intent: json!({"goal":"Whole authority."}),
        trigger: packet["trigger"].clone(),
        documents: vec![],
        created_ms: 1,
    };
    state.pending = Some(Pending::from(&request));
    store.save(&state).unwrap();
    let requests = store.directory.join("requests");
    fs::create_dir_all(&requests).unwrap();
    source_store::save(
        &requests,
        &format!("{}.json", request.id),
        &serde_json::to_value(&request).unwrap(),
    )
    .unwrap();
    let input = &request.trigger["preparation"]["state"];
    preparation::stage(
        &request,
        input,
        &Default::default(),
        "retained provider rejection",
    )
    .unwrap();
    let job = preparation::job(
        &request,
        input,
        &Default::default(),
        "retained provider rejection",
    );
    let manifest = manifest(&job);
    for _ in 0..2 {
        preparation::rejection::record(
            &request,
            &job,
            &manifest,
            input,
            "JEV_CONTEXT_LIMIT stage=judge_http http_status=400",
        )
        .unwrap();
    }
    source_store::save(
        &store.directory,
        &format!("selection-{}.json", request.id),
        &serde_json::to_value(manifest).unwrap(),
    )
    .unwrap();
    store
}
