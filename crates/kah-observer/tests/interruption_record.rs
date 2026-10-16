use kah_core::{observation::ObservationSnapshot, run_store::RunStore};
use kah_observer::{
    engine,
    judge::routing::Backend,
    session::{Binding, Flight, State, Store, process_stamp},
};
use serde_json::Value;
use std::{fs, path::PathBuf, time::Duration};

fn dead_flight() -> Flight {
    let mut child = std::process::Command::new("sleep")
        .arg("30")
        .spawn()
        .unwrap();
    let pid = child.id();
    let stamp = process_stamp(pid).unwrap();
    child.kill().unwrap();
    child.wait().unwrap();
    Flight {
        pid,
        process_stamp: stamp,
        turn: "killed".into(),
        started_ms: 0,
    }
}

#[test]
fn an_interrupted_observation_is_journaled_before_the_next_one_clears_it() {
    // C7(b) phase 3: the next observation recovered OBSERVER_INTERRUPTED and
    // its success cleared the gap, leaving no record of the interruption.
    let id = format!("interrupted-{}", std::process::id());
    let project = std::env::temp_dir().join(format!("kah-{id}"));
    fs::create_dir_all(&project).unwrap();
    let binding = Binding {
        session: id.clone(),
        project: project.clone(),
        codex_home: PathBuf::from("/unused"),
        native: PathBuf::from("/unused"),
        model: "gpt-6.1-sol".into(),
        effort: "low".into(),
        timeout_secs: 1,
        judge: Backend::Jev,
        jev_model: "jev-latest".into(),
    };
    let base = RunStore::default_state_root().unwrap();
    let mut state = State::new(binding.clone());
    state.inflight = Some(dead_flight());
    let mut store = Store::open(&base, &id, Duration::ZERO).unwrap();
    store.save(&state).unwrap();
    drop(store);
    let empty = ObservationSnapshot {
        native_candidates: None,
        users: None,
        actions: vec![],
        action_digest: {
            use sha2::{Digest, Sha256};
            format!("sha256:{:x}", Sha256::digest(b"[]"))
        },
    };
    // Recording happens before the observation runs, whatever its outcome.
    let outcome = engine::synchronize_snapshot(&binding, "next", empty).map(|_| ());
    let directory = Store::directory(&base, &id);
    let journal = fs::read_to_string(directory.join("observations.jsonl")).unwrap_or_default();
    let gaps: Vec<Value> = journal
        .lines()
        .map(|l| serde_json::from_str::<Value>(l).unwrap())
        .filter(|r| r["kind"] == "gap" && r["source"] == "observer")
        .collect();
    fs::remove_dir_all(&directory).unwrap();
    fs::remove_dir_all(project).unwrap();
    assert_eq!(gaps.len(), 1, "{outcome:?} {journal}");
    assert_eq!(gaps[0]["gap"]["code"], "OBSERVER_INTERRUPTED");
    assert!(gaps[0]["gap"]["at_ms"].as_u64().is_some_and(|t| t > 0));
}
