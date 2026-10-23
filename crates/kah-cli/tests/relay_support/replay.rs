//! Same native observer lifecycle and Jev decisions over recorded evidence.
use super::*;
use kah_cli::observer_review::{
    snapshot,
    state::{Purpose, Request, State, Store as Reviews},
};
#[path = "replay_cases.rs"]
mod cases;

pub fn run(root: &Path) -> Value {
    assert!(
        std::env::var("HERDR_SOCKET_PATH")
            .unwrap()
            .contains("kah-gate-")
    );
    let home = PathBuf::from(std::env::var_os("HOME").unwrap());
    let (launch, mut fixture) = crate::fixture::prepare(&home, "research", false);
    let mut command = launch.command(false);
    command
        .args(["--no-alt-screen", BLUE])
        .env_remove("CODEX_THREAD_ID");
    let mut panes = Panes(vec![Pane::start(&command).unwrap()]);
    let worker = ready(&panes.0[0], "codex");
    fixture.sessions.push(worker.clone());
    let cleanup = super::review::Cleanup {
        home: home.clone(),
        worker: worker.clone(),
    };
    let mut outcomes = vec![];
    let mut observer = None;
    for case in cases::load(root) {
        let outcome = review_case(&home, &worker, &case, &mut panes);
        let native = Reviews::read(&worker)
            .unwrap()
            .unwrap()
            .observer
            .session
            .unwrap();
        if let Some(expected) = &observer {
            assert_eq!(&native, expected);
        } else {
            observer = Some(native.clone());
        }
        let entry = json!({"case":case.name,"provenance":case.provenance,"observerSession":native,"result":outcome});
        let path = PathBuf::from(std::env::var_os("KAH_GATE_EVIDENCE").unwrap())
            .join(format!("replay-{}.json", case.name));
        assert!(!path.exists(), "never overwrite a prior replay outcome");
        fs::write(path, serde_json::to_vec_pretty(&entry).unwrap()).unwrap();
        outcomes.push(entry);
    }
    drop(panes);
    drop(cleanup);
    json!({"versions":versions(&home),"models":{"codex":"gpt-6.1-sol/low"},"historicalReplay":outcomes,"sameObserverSession":observer,"thresholdsUnchanged":true,"privateHerdr":true,"passed":true,"root":root})
}
fn review_case(home: &Path, worker: &str, case: &cases::Case, panes: &mut Panes) -> Value {
    let store = idle_store(worker, panes);
    let state = store.load().unwrap().unwrap_or_else(|| {
        State::new(kah_cli::relay_store::endpoint(worker).unwrap(), home).unwrap()
    });
    assert!(state.pending.is_none());
    let request = Request {
        id: format!(
            "review-{}-{}",
            kah_observer::session::now_ms(),
            std::process::id()
        ),
        worker: worker.into(),
        purpose: Purpose::Drift,
        turn: case.name.clone(),
        head: None,
        intent: case.intent.clone(),
        // The complete digest inventory belongs to gate audit evidence below,
        // not to the behavioral decision. The brain still reads every whole
        // source record and its complete chronological evaluation scope.
        trigger: json!({"historical_replay":{"source":case.provenance["source"],"interval_start":case.provenance["intervalStart"],"interval_end":case.provenance["intervalEnd"],"counterfactual":case.provenance["counterfactual"]},"subject":"Recorded builder actions; the disposable transport worker is not the evaluation subject.","proportion_review_required":case.name=="auth-actual"}),
        documents: case.documents.clone(),
        created_ms: kah_observer::session::now_ms(),
    };
    drop(store);
    let receipt = snapshot::wake(&request).unwrap();
    let receipt = if receipt["pane"].is_string() {
        receipt
    } else {
        loop {
            let current = Reviews::read(worker).unwrap().unwrap();
            if current.pending.as_ref().is_some_and(|p| p.id == request.id)
                || current
                    .latest
                    .as_ref()
                    .is_some_and(|v| v["request"] == request.id)
            {
                break json!({"pane":current.observer.pane});
            }
            kah_cli::observer_review::queue::drain(worker).unwrap();
            std::thread::sleep(Duration::from_millis(200));
        }
    };
    let pane = Pane {
        id: receipt["pane"].as_str().unwrap().into(),
    };
    if !panes.0.iter().any(|p| p.id == pane.id) {
        panes.0.push(pane.clone());
    }
    let reviewed = super::review::wait_review(worker, &pane, "codex");
    assert_eq!(reviewed.latest.as_ref().unwrap()["request"], request.id);
    read_result(worker, case, reviewed.latest.unwrap())
}
fn read_result(worker: &str, case: &cases::Case, latest: Value) -> Value {
    let result: Value = kah_cli::observer_review::source_store::read(
        &Reviews::directory(worker)
            .unwrap()
            .join("results")
            .join(format!("{}.json", latest["request"].as_str().unwrap())),
    )
    .unwrap();
    if case.name == "auth-actual" {
        assert!(
            result["report"]["decision_units"]["proportion"].is_object(),
            "auth replay needs the thinker's cost estimates and Jev proportion decision"
        );
    }
    result
}
fn idle_store(worker: &str, panes: &mut Panes) -> Reviews {
    loop {
        match Reviews::open(worker) {
            Ok(store) => {
                let state = store.load().unwrap();
                if state.as_ref().is_none_or(|s| s.pending.is_none()) {
                    return store;
                }
                let observer = state.unwrap().observer;
                drop(store);
                if let Some(id) = observer.pane {
                    let pane = Pane { id };
                    if !panes.0.iter().any(|p| p.id == pane.id) {
                        panes.0.push(pane.clone());
                    }
                    super::review::wait_review(worker, &pane, "codex");
                }
            }
            Err(e) if e.to_string().starts_with("REVIEW_BUSY") => {}
            Err(e) => panic!("{e}"),
        }
        std::thread::sleep(Duration::from_millis(200));
    }
}
