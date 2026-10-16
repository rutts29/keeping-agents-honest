use super::*;
use crate::observer_review::state::{Pending, Purpose, State};

fn config(pid: u32, stamp: Option<String>) -> control::Config {
    control::Config {
        enabled: true,
        provider: "codex".into(),
        worker: "worker".into(),
        turn: "turn".into(),
        pid: Some(pid),
        process_stamp: stamp,
    }
}

fn exited_child() -> (u32, String) {
    let mut child = std::process::Command::new("sleep")
        .arg("30")
        .spawn()
        .unwrap();
    let pid = child.id();
    let stamp = kah_observer::session::process_stamp(pid).unwrap();
    child.kill().unwrap();
    child.wait().unwrap();
    (pid, stamp)
}

#[test]
fn only_the_exact_exited_worker_process_ends_supervision() {
    assert!(ended_worker(None).is_none());
    let own = std::process::id();
    let stamp = kah_observer::session::process_stamp(own);
    assert!(ended_worker(Some(&config(own, stamp))).is_none());
    assert!(ended_worker(Some(&config(own, None))).is_none());
    let (pid, stamp) = exited_child();
    assert_eq!(
        ended_worker(Some(&config(pid, Some(stamp.clone())))),
        Some((pid, stamp))
    );
}

fn setup(worker: &str, pid: u32, stamp: &str) {
    use kah_observer::session;
    let base = kah_core::run_store::RunStore::default_state_root().unwrap();
    let home = base.join(worker);
    let binding = session::Binding {
        session: worker.into(),
        project: home.clone(),
        codex_home: home.join("codex"),
        native: "/nonexistent/native".into(),
        model: "gpt-6.1-sol".into(),
        effort: "low".into(),
        timeout_secs: 90,
        judge: Default::default(),
        jev_model: "jev-latest".into(),
    };
    let mut primary = session::Store::open(&base, worker, std::time::Duration::ZERO).unwrap();
    primary.save(&session::State::new(binding)).unwrap();
    drop(primary);
    let endpoint = crate::relay_store::Endpoint {
        provider: "codex".into(),
        session: worker.into(),
        project: home.clone(),
        pane: Some("w0:gone-worker".into()),
        socket: Some(home.join("absent.sock")),
    };
    let mut state = State::new(endpoint, &home).unwrap();
    // A failed cold delivery leaves this shape: pending, pane recorded, never
    // stopped, and no lifecycle watcher.
    state.observer.pane = Some("w0:gone-observer".into());
    state.pending = Some(Pending {
        id: "review-orphan".into(),
        purpose: Purpose::Drift,
        turn: "turn".into(),
        head: None,
        created_ms: 0,
        native_tokens_before: None,
    });
    Store::open(worker).unwrap().save(&state).unwrap();
    let mut registered = config(pid, Some(stamp.into()));
    registered.worker = worker.into();
    crate::unattended_report::save(
        &Store::directory(worker).unwrap(),
        "config.json",
        &serde_json::to_value(registered).unwrap(),
    )
    .unwrap();
}

#[test]
fn watcher_exits_and_stops_the_review_when_its_worker_already_exited() {
    let worker = format!("liveness-orphan-{}", std::process::id());
    let (pid, stamp) = exited_child();
    setup(&worker, pid, &stamp);
    let (sender, receiver) = std::sync::mpsc::channel();
    let watched = worker.clone();
    std::thread::spawn(move || sender.send(watch(&watched).map_err(|e| e.to_string())));
    let outcome = receiver
        .recv_timeout(std::time::Duration::from_secs(60))
        .expect("watcher outlived its exited worker");
    outcome.unwrap();
    let state = Store::read(&worker).unwrap().unwrap();
    assert!(state.stopped, "worker exit marks the review stopped");
    assert!(state.observer.pane.is_none());
    let root = Store::directory(&worker).unwrap();
    std::fs::remove_dir_all(root.parent().unwrap()).unwrap();
}
