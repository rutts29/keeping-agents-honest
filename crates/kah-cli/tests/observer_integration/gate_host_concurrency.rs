//! Gate-host interleaving on authored requests, reports and typed judgments.
//! No provider client or inference entrypoint is used by these transports.
use super::{
    evaluation::{self, Evaluation},
    *,
};
use crate::observer_review::{consume, decisions::scoped, preparation, queue};
use kah_core::run_store::RunStore;
use kah_observer::{
    judge::{Answer, Question, Questions},
    review_evidence::{Citation, Document, Finding, Topic},
    session,
};
use std::{
    collections::BTreeMap,
    io::Write,
    os::unix::fs::OpenOptionsExt,
    process::Command,
    sync::{Arc, Barrier, mpsc},
};
const GATE: &str = "review-1700000600000-601";
const SECOND: &str = "review-1700000600100-602";
const COMMIT: &str = "review-1700000600200-603";
const GATE_HEAD: &str = "a1b2c3d4e5f60718293a4b5c6d7e8f9011223344";
const COMMIT_HEAD: &str = "b2c3d4e5f60718293a4b5c6d7e8f901122334455";
const PLAN: &str = "# Plan\n\n1. Add the word counter.\n2. Check it on the delivered head.\n";
/// The decision keys a gate evaluation asks one by one.
const KEYS: [&str; 4] = [
    "requirement.counter",
    "requirement.check",
    "applicability.counter",
    "applicability.check",
];

fn judgment(keys: &[&str]) -> Judgment {
    let answers = keys
        .iter()
        .map(|key| {
            let answer = Answer::Choice {
                choice: "yes".into(),
                probabilities: BTreeMap::from([("yes".into(), 0.97), ("no".into(), 0.03)]),
                confidence: 0.95,
            };
            ((*key).to_owned(), answer)
        })
        .collect();
    Judgment {
        backend: "jev".into(),
        model: "jev-1.13.0".into(),
        answers,
        metrics: Default::default(),
        fallback_reason: None,
    }
}
fn request(worker: &str, id: &str, head: &str, commit: bool) -> Request {
    let trigger = if commit {
        json!({"review_class":"commit","packet_scope":"diff-v1","head":head,
            "events":[{"kind":"native_head","review_class":"commit","head":head}]})
    } else {
        json!({"review_class":"gate","events":[{"kind":"user_gate","source":"host_cli"}]})
    };
    Request {
        id: id.into(),
        worker: worker.into(),
        purpose: Purpose::Checkpoint,
        turn: "t".into(),
        head: Some(head.into()),
        intent: json!({"goal":"Add a word counter and check it.","revision":2}),
        trigger,
        documents: vec![Document::new("plan", "plan", "plan:PLAN.md", PLAN)],
        created_ms: 1,
    }
}
fn report(request: &Request) -> Report {
    let citation = Citation {
        source: "plan".into(),
        digest: request.documents[0].digest.clone(),
        quote: "Add the word counter.".into(),
        occurrence: 0,
    };
    Report {
        request_id: request.id.clone(),
        findings: vec![Finding {
            topic: Topic::Intent,
            claim: "The plan names the word counter as this delivery.".into(),
            citations: vec![citation],
            deferral: None,
            checkpoint_scope: None,
        }],
        reminder: None,
        decision_units: None,
        verification_risks: None,
        verification_contracts: None,
        component_map: None,
    }
}
fn decision(request: &Request) -> Evaluation {
    Evaluation {
        judgment: judgment(&KEYS),
        input: json!({"request":request.id,"head":request.head}),
        assessment: json!({"passed":true,"head":request.head,"unproven":[]}),
    }
}

struct Fixture {
    worker: String,
    observer: String,
}
impl Fixture {
    fn new(label: &str) -> Self {
        std::env::var_os("XDG_STATE_HOME").expect("isolated check-tests fixture required");
        let worker = format!("gate-{label}-{}", crate::relay_store::nonce());
        let base = RunStore::default_state_root().unwrap();
        kah_core::run_store::ensure_private_directory(&session::Store::directory(&base, &worker))
            .unwrap();
        let store = Store::open(&worker).unwrap();
        let state: State = serde_json::from_value(json!({
            "worker":{"provider":"codex","session":worker,"project":store.directory.join("worker"),"pane":null,"socket":null},
            "observer":{"session":"observer-one","pane":null,"name":"fixture","workspace":store.directory.join("observer"),
                "model":"gpt-6.1-sol","effort":"low","transcript":null},
            "pending":null,"latest":null,"last_verified_turn":null,"checkpoint":null,"deferrals":[],
            "wakes":0,"native_tokens":0,"stopped":false})).unwrap();
        kah_core::run_store::ensure_private_directory(&state.observer.workspace.join("outbox"))
            .unwrap();
        store.save(&state).unwrap();
        drop(store);
        let binding = session::Binding {
            session: worker.clone(),
            project: state.worker.project.clone(),
            codex_home: state.worker.project.join("codex"),
            native: "codex".into(),
            model: "gpt-6.1-sol".into(),
            effort: "low".into(),
            timeout_secs: 90,
            judge: Default::default(),
            jev_model: "jev-latest".into(),
        };
        session::Store::open_wait(&base, &worker)
            .unwrap()
            .save(&session::State::new(binding))
            .unwrap();
        Self {
            worker,
            observer: "observer-one".into(),
        }
    }
    fn request(&self, id: &str, head: &str, commit: bool) -> Request {
        request(&self.worker, id, head, commit)
    }
    fn ack(&self, request: &Request) -> Job {
        let report = report(request);
        let actions = report
            .verify(&request.id, &request.documents)
            .unwrap()
            .into_iter()
            .enumerate()
            .map(|(i, v)| json!({"id":format!("verified-{i}"),"kind":"verifiedEvidence","evidence":v}))
            .collect::<Vec<_>>();
        let store = Store::open(&self.worker).unwrap();
        let mut state = store.load().unwrap().unwrap();
        assert!(state.pending.is_none());
        state.pending = Some(Pending::from(request));
        state.wakes += 1;
        source_store::save(
            &store.directory.join("requests"),
            &format!("{}.json", request.id),
            &serde_json::to_value(request).unwrap(),
        )
        .unwrap();
        let outbox = state.observer.workspace.join("outbox");
        let name = format!("submitted-{}.json", request.id);
        crate::unattended_report::save(&outbox, &name, &json!(report)).unwrap();
        let ready = json!({"request":request.id});
        crate::unattended_report::save(&store.directory, "report-ready.json", &ready).unwrap();
        store.save(&state).unwrap();
        assert!(crate::observer_review::liveness::handoff::awaiting_host(&state).unwrap());
        Job {
            request: request.clone(),
            pending: state.pending.clone().unwrap(),
            state,
            report,
            actions,
            submitted: outbox.join(name),
            report_revision: None,
            native_tokens_end: None,
            usage_gap: Some("authored transport; no native usage".into()),
        }
    }
    fn handoff(&self, request: &Request) -> Job {
        let job = self.ack(request);
        let store = Store::open(&self.worker).unwrap();
        let bytes = consume::read_report(&job.submitted).unwrap();
        let submission = super::super::report_revision::Submission::capture(
            &store.directory,
            &job.submitted,
            &bytes,
        )
        .unwrap();
        let mut state = store.load().unwrap().unwrap();
        fs::remove_file(store.directory.join("report-ready.json")).unwrap();
        let usage = || Err("AUTHORED_NATIVE_USAGE stage=authored_transport".into());
        let error = handoff_locked(
            &store,
            &mut state,
            request,
            &job.report,
            &job.actions,
            &submission,
            usage(),
        )
        .unwrap_err();
        assert!(
            error
                .to_string()
                .starts_with("REVIEW_SUBMISSION_UNCONFIRMED")
        );
        assert_eq!(state.pending.as_ref().unwrap().id, request.id);
        let ready = json!({"request":request.id});
        crate::unattended_report::save(&store.directory, "report-ready.json", &ready).unwrap();
        let saved = handoff_locked(
            &store,
            &mut state,
            request,
            &job.report,
            &job.actions,
            &submission,
            usage(),
        );
        assert!(saved.unwrap());
        assert!(store.load().unwrap().unwrap().pending.is_none());
        let root = directory(&self.worker, &request.id).unwrap();
        source_store::read(&root.join("job.json")).unwrap()
    }
}

#[test]
fn gate_and_commit_interleave_without_changing_the_gate_verdict() {
    let alone = Fixture::new("alone");
    let request = alone.request(GATE, GATE_HEAD, false);
    let job = alone.handoff(&request);
    let isolated = evaluation::process_with(&job, |job| Ok(decision(&job.request))).unwrap();
    let fixture = Fixture::new("between");
    let request = fixture.request(GATE, GATE_HEAD, false);
    let gate = fixture.handoff(&request);
    assert_handoff_outcome(&fixture, &request);
    let (entered_tx, entered_rx) = mpsc::channel();
    let (continue_tx, continue_rx) = mpsc::channel();
    let driver = std::thread::spawn(move || {
        evaluation::process_with(&gate, |job| {
            entered_tx.send(()).unwrap();
            continue_rx.recv().unwrap();
            Ok(decision(&job.request))
        })
        .unwrap()
    });
    entered_rx.recv().unwrap(); // The gate evaluator owns no main Store here.
    let commit = fixture.request(COMMIT, COMMIT_HEAD, true);
    let commit_job = admit_commit(&fixture, &commit);
    let result = complete_commit(&commit_job, &decision(&commit));
    let gate_root = directory(&fixture.worker, GATE).unwrap();
    assert!(!gate_root.join("finished.json").exists());
    assert_eq!(result["head"], json!(commit.head));
    assert_eq!(result["request"], commit.id);
    let saved = Store::read(&fixture.worker).unwrap().unwrap();
    assert!(saved.pending.is_none());
    continue_tx.send(()).unwrap();
    let interleaved = driver.join().unwrap();
    assert_eq!(interleaved["assessment"], isolated["assessment"]);
    assert_eq!(interleaved["judgment"], isolated["judgment"]);
    assert_eq!(interleaved["head"], json!(request.head));
    assert_eq!(interleaved["request"], request.id);
}
fn assert_handoff_outcome(fixture: &Fixture, request: &Request) {
    consume::outcome::finished(&fixture.worker, Some(request.id.clone()), 0, &Ok(())).unwrap();
    let path = Store::directory(&fixture.worker)
        .unwrap()
        .join("consume-outcomes.jsonl");
    let events = fs::read_to_string(path).unwrap();
    let last: Value = serde_json::from_str(events.lines().last().unwrap()).unwrap();
    assert!(
        last["event"]["reason"]
            .as_str()
            .unwrap()
            .starts_with("REVIEW_CONSUME_HANDOFF")
    );
}
fn admit_commit(fixture: &Fixture, request: &Request) -> Job {
    let trigger = json!({"captured_snapshot":request});
    queue::enqueue(
        &fixture.worker,
        request.purpose.clone(),
        &request.turn,
        trigger,
    )
    .unwrap();
    let (_, paths) = queue::claim_batch(&fixture.worker).unwrap().unwrap();
    assert_eq!(paths.len(), 1);
    fixture.ack(request)
}
fn complete_commit(job: &Job, decision: &Evaluation) -> Value {
    assert!(crate::observer_review::commit_packet::is_commit(
        &job.request
    ));
    let store = Store::open(&job.request.worker).unwrap();
    let mut state = store.load().unwrap().unwrap();
    assert_eq!(state.pending.as_ref().unwrap().id, job.request.id);
    let result = json!({"request":job.request.id,"purpose":job.request.purpose,"head":job.request.head,
        "report":job.report,"jev_state":decision.input,"judgment":decision.judgment,"assessment":decision.assessment});
    let version = consume::question_version(&job.request, &job.report);
    let (input, judgment) = (&decision.input, &decision.judgment);
    consume::persist_evidence(
        &store,
        &job.request,
        &job.report,
        &result,
        input,
        judgment,
        version,
    )
    .unwrap();
    consume::timed::record_effects(&store, &state, &job.request, &job.report, &result, judgment)
        .unwrap();
    consume::commit_result(&store, &mut state, &job.request, result.clone(), None).unwrap();
    result
}

#[test]
fn restart_reuses_completed_decisions_after_a_real_process_exit() {
    if let Some(root) = std::env::var_os("KAH_GATE_RESTART_JOB") {
        restart_child(&PathBuf::from(root));
        return;
    }
    let fixture = Fixture::new("restart");
    let request = fixture.request(GATE, GATE_HEAD, false);
    let job = fixture.handoff(&request);
    let root = directory(&fixture.worker, &request.id).unwrap();
    let first = child(&root, "2");
    assert_eq!(first.code(), Some(73));
    let before = calls(&root);
    assert_eq!(before.len(), 2);
    assert!(!root.join("finished.json").exists());
    assert!(Store::open(&fixture.worker).is_ok()); // OS locks released by exit.
    assert!(child(&root, "0").success());
    let after = calls(&root);
    let unique = after.iter().collect::<std::collections::BTreeSet<_>>();
    assert_eq!(after.len(), unique.len(), "no decision is asked twice");
    assert_eq!(after.len(), KEYS.len());
    assert_eq!(&after[..before.len()], before.as_slice());
    let result: Value = source_store::read(&root.join("finished.json")).unwrap();
    assert_eq!(result["request"], job.request.id);
    assert_eq!(
        result["judgment"]["answers"],
        json!(judgment(&KEYS).answers)
    );
    assert!(child(&root, "0").success());
    assert_eq!(
        calls(&root).len(),
        KEYS.len(),
        "terminal retry repeats no decisions"
    );
}
fn child(root: &Path, stop: &str) -> std::process::ExitStatus {
    let name = "observer_review::gate_host::tests::restart_reuses_completed_decisions_after_a_real_process_exit";
    Command::new(std::env::current_exe().unwrap())
        .args(["--exact", name, "--nocapture"])
        .env("KAH_GATE_RESTART_JOB", root)
        .env("KAH_GATE_RESTART_STOP_AFTER", stop)
        .status()
        .unwrap()
}
fn restart_child(root: &Path) {
    let job: Job = source_store::read(&root.join("job.json")).unwrap();
    evaluation::process_with(&job, |job| {
        Ok(Evaluation {
            judgment: decide_each(&job.request, root),
            ..decision(&job.request)
        })
    })
    .unwrap();
}
/// Asks one durable scoped decision per key; may exit like a host crash.
fn decide_each(request: &Request, root: &Path) -> Judgment {
    let stop: usize = std::env::var("KAH_GATE_RESTART_STOP_AFTER")
        .unwrap()
        .parse()
        .unwrap();
    let mut combined = judgment(&[]);
    combined.backend = "not_requested".into();
    for key in KEYS {
        let input = json!({"request":request.id,"key":key});
        let instructions = format!("Is {key} satisfied on the delivered head?");
        let questions = Questions::from([(key.to_owned(), Question::Noul { instructions })]);
        let (decided, reused) = scoped::decide_with(request, &input, &questions, |_| {
            record_call(root, key).unwrap();
            Ok((judgment(&[key]), input.clone(), json!("authored")))
        })
        .unwrap();
        scoped::merge(&mut combined, &decided.0, reused).unwrap();
        if stop != 0 && calls(root).len() == stop {
            std::process::exit(73); // Host interruption after a durable decision write.
        }
    }
    combined
}
fn record_call(root: &Path, key: &str) -> Result<()> {
    let mut file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .mode(0o600)
        .open(root.join("recorded-decisions.jsonl"))?;
    writeln!(file, "{}", serde_json::to_string(key)?)?;
    Ok(file.sync_all()?)
}
fn calls(root: &Path) -> Vec<String> {
    fs::read_to_string(root.join("recorded-decisions.jsonl"))
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect()
}

#[test]
fn concurrent_merges_preserve_each_job_and_a_third_pending_review() {
    let fixture = Fixture::new("concurrent");
    let first = fixture.request(GATE, GATE_HEAD, false);
    let first_job = fixture.handoff(&first);
    let second = fixture.request(SECOND, GATE_HEAD, false);
    let second_job = fixture.handoff(&second);
    let commit = fixture.request(COMMIT, COMMIT_HEAD, true);
    let commit_job = fixture.ack(&commit);
    for id in [&first.id, &second.id, &commit.id] {
        preparation::cursors::activate(&fixture.worker, id, &format!("{id}-context")).unwrap();
    }
    let barrier = Arc::new(Barrier::new(3));
    let drivers = [first_job, second_job]
        .into_iter()
        .map(|job| {
            let barrier = barrier.clone();
            std::thread::spawn(move || {
                evaluation::process_with(&job, |job| {
                    barrier.wait();
                    Ok(decision(&job.request))
                })
                .unwrap()
            })
        })
        .collect::<Vec<_>>();
    barrier.wait();
    for driver in drivers {
        driver.join().unwrap();
    }
    assert_concurrent_outcomes(&fixture, &commit_job.pending, [&first, &second]);
}
fn assert_concurrent_outcomes(fixture: &Fixture, pending: &Pending, finished: [&Request; 2]) {
    let saved = Store::read(&fixture.worker).unwrap().unwrap();
    assert_eq!(
        serde_json::to_value(saved.pending).unwrap(),
        serde_json::to_value(Some(pending)).unwrap()
    );
    assert_eq!(
        saved.observer.session.as_deref(),
        Some(fixture.observer.as_str())
    );
    for request in finished {
        let root = directory(&fixture.worker, &request.id).unwrap();
        let result: Value = source_store::read(&root.join("finished.json")).unwrap();
        assert_eq!(result["head"], json!(request.head));
        assert_eq!(result["request"], request.id);
        assert_eq!(
            preparation::active_decision(&fixture.worker, &request.id).unwrap(),
            format!("{}-context", request.id)
        );
    }
    assert_eq!(
        preparation::active_decision(&fixture.worker, &pending.id).unwrap(),
        format!("{}-context", pending.id)
    );
}
