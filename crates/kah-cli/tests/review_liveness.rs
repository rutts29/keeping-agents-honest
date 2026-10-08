use kah_cli::observer_review::liveness::{Progress, Sample, transition};

fn sample(now: u64) -> Sample {
    Sample {
        request: "pending".into(),
        now_ms: now,
        started_ms: 0,
        native_active: false,
        controller_active: false,
        host_owned: false,
        transcript_version: None,
        report_version: None,
        cause: "idle pane".into(),
    }
}

#[test]
fn acknowledged_host_work_needs_real_progress_and_never_requests_native_redelivery() {
    let mut current = sample(0);
    current.host_owned = true;
    current.cause = "host consume waiting for a continuation".into();
    current.report_version = Some((1, 0));
    let first = transition(None, &current);
    current.now_ms = 300_000;
    let stalled = transition(Some(&first), &current);
    assert!(stalled.stalled);
    assert_eq!(
        kah_cli::observer_review::liveness::description(&current).0,
        "host-stalled"
    );
    assert!(
        kah_cli::observer_review::liveness::description(&current)
            .1
            .contains("no native redelivery")
    );
    assert!(!transition(Some(&stalled), &current).changed);
    current.report_version = Some((2, current.now_ms));
    assert!(!transition(Some(&stalled), &current).stalled);
}

#[test]
fn outside_packet_expansion_is_host_progress_not_observer_failure() {
    for cause in [
        "HOST EXPANSION: REVIEW_SOURCE_OUTSIDE_PACKET stage=report",
        "VALIDATION BLOCKED: REVIEW_SOURCE_OUTSIDE_PACKET stage=report",
    ] {
        let mut current = sample(0);
        current.cause = cause.into();
        current.controller_active = true;
        let previous = transition(None, &current);
        current.now_ms = 3_600_000;
        assert!(!transition(Some(&previous), &current).stalled);
        assert_eq!(
            kah_cli::observer_review::liveness::description(&current).0,
            "host-expansion"
        );
    }
}

#[test]
fn idle_hung_publish_crash_and_stuck_lock_become_stalls_without_a_review_deadline() {
    for cause in [
        "idle pane",
        "hung pane without native activity",
        "published but not delivered",
        "review lock busy",
    ] {
        let mut observation = sample(0);
        observation.cause = cause.into();
        let initial = transition(None, &observation);
        observation.now_ms = 15_000;
        let stalled = transition(Some(&initial), &observation);
        assert!(stalled.stalled, "{cause}");
        assert!(stalled.changed);
        assert!(
            !transition(Some(&stalled), &observation).changed,
            "same stall must not spam"
        );
    }
}

#[test]
fn progressing_native_or_controller_work_is_never_expired_and_recovery_is_recorded() {
    let initial = transition(None, &sample(0));
    let stalled = transition(Some(&initial), &sample(15_000));
    for native in [true, false] {
        let mut active = sample(3_600_000);
        active.native_active = native;
        active.controller_active = !native;
        if native {
            active.transcript_version = Some((100, active.now_ms));
        }
        let recovered = transition(Some(&stalled), &active);
        assert!(!recovered.stalled);
        assert!(recovered.changed);
        assert!(!transition(Some(&recovered), &active).stalled);
    }
}

#[test]
fn working_without_new_records_becomes_a_human_notice_and_growth_recovers_it() {
    let mut active = sample(0);
    active.native_active = true;
    active.transcript_version = Some((100, 0));
    let first = transition(None, &active);
    active.now_ms = 300_000;
    let hung = transition(Some(&first), &active);
    assert!(hung.stalled && hung.changed);
    assert!(
        hung.sample.native_active,
        "Working hang only notifies; no auto-interrupt"
    );
    active.now_ms += 1000;
    active.transcript_version = Some((200, active.now_ms));
    assert!(!transition(Some(&hung), &active).stalled);
}

#[test]
fn native_transcript_or_report_growth_resets_only_the_no_progress_observation() {
    let mut observation = sample(0);
    let initial: Progress = transition(None, &observation);
    observation.now_ms = 30_000;
    observation.report_version = Some((40, 30_000));
    let growing = transition(Some(&initial), &observation);
    assert!(!growing.stalled);
    observation.now_ms = 40_000;
    assert!(!transition(Some(&growing), &observation).stalled);
    observation.now_ms = 45_000;
    assert!(transition(Some(&growing), &observation).stalled);
}

#[test]
fn actual_held_review_lock_cannot_hide_stall_notice_or_pending_request() {
    use kah_cli::observer_review::{liveness, state::Store};
    let worker = format!("liveness-lock-{}", std::process::id());
    let state = setup(&worker);
    let held = Store::open(&worker).unwrap();
    held.save(&state).unwrap();
    liveness::run(&worker).unwrap();
    let root = held.directory.join("liveness");
    assert!(
        liveness::notice(&worker).unwrap().is_none(),
        "first observation is a handoff baseline"
    );
    let path = root.join("progress.json");
    let mut prior: Progress = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    prior.last_progress_ms = 0;
    prior.sample.cause = "fixture awaiting actual resample".into();
    std::fs::write(&path, serde_json::to_vec(&prior).unwrap()).unwrap();
    // Sampling intentionally serializes nonblocking. A concurrent test's
    // fork can briefly inherit the lock before exec closes descriptors.
    // Observe an actual new sample before asserting its unchanged stall rule.
    let progress: Progress = loop {
        liveness::run(&worker).unwrap();
        let current: Progress = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        if current.sample.cause != "fixture awaiting actual resample" {
            break current;
        }
        std::thread::yield_now();
    };
    assert!(progress.stalled);
    assert_eq!(progress.sample.cause, "review lock busy");
    let status = kah_cli::observer_review::control::status(&worker).unwrap();
    assert_eq!(status["liveness"]["progress"]["stalled"], true);
    assert!(
        status["liveness"]["journal"]
            .as_str()
            .unwrap()
            .ends_with("liveness/events.jsonl")
    );
    let notice = liveness::notice(&worker).unwrap().unwrap();
    assert!(
        notice["systemMessage"]
            .as_str()
            .unwrap()
            .contains("review-stall")
    );
    assert_eq!(
        Store::read(&worker).unwrap().unwrap().pending.unwrap().id,
        "review-stall"
    );
    assert!(
        root.join("reviewer.json").exists(),
        "unavailable reviewer delivery remains durable"
    );
    drop(held);
    cleanup_jobs(&worker);
}

#[test]
fn previous_completes_next_queued_and_briefly_busy_lock_is_not_a_stall() {
    let previous = transition(None, &sample(0));
    let mut next = sample(3_600_000);
    next.request = "next-review".into();
    next.cause = "review lock busy".into();
    let baseline = transition(Some(&previous), &next);
    assert!(!baseline.stalled);
    next.now_ms += 10_000;
    assert!(!transition(Some(&baseline), &next).stalled);
    next.native_active = true;
    next.now_ms += 20_000;
    assert!(!transition(Some(&baseline), &next).stalled);
}

#[test]
fn processing_marker_protects_real_progress_then_publish_crash_is_visible() {
    use kah_cli::observer_review::{liveness, state::Store};
    let worker = format!("liveness-processing-{}", std::process::id());
    let mut state = setup(&worker);
    let store = Store::open(&worker).unwrap();
    store.save(&state).unwrap();
    drop(store);
    let processing = liveness::processing(&worker, "review-stall").unwrap();
    let nested = liveness::processing(&worker, "review-stall").unwrap();
    drop(nested);
    liveness::run(&worker).unwrap();
    assert!(liveness::notice(&worker).unwrap().is_none());
    drop(processing);
    let path = Store::directory(&worker)
        .unwrap()
        .join("liveness/progress.json");
    let mut prior: Progress = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    prior.last_progress_ms = 0;
    std::fs::write(&path, serde_json::to_vec(&prior).unwrap()).unwrap();
    liveness::run(&worker).unwrap();
    assert!(
        liveness::notice(&worker).unwrap().unwrap()["systemMessage"]
            .as_str()
            .unwrap()
            .contains("stalled")
    );
    state.pending = None;
    // Liveness may have started a detached recovery holding the store lock.
    // Use the existing controller lock wait before recording recovery.
    let store = Store::open_wait(&worker).unwrap();
    store.save(&state).unwrap();
    drop(store);
    liveness::run(&worker).unwrap();
    assert!(
        liveness::notice(&worker).unwrap().unwrap()["systemMessage"]
            .as_str()
            .unwrap()
            .contains("recovered")
    );
    cleanup_jobs(&worker);
}

fn cleanup_jobs(worker: &str) {
    let root = kah_cli::observer_review::state::Store::directory(worker).unwrap();
    // Detached jobs now execute real kah, rather than accidentally starting the
    // test runner. Their inherited OS lock proves they exited before deletion.
    for job in ["__review-retry", "__review-consume", "__review-hook-events"] {
        let path = root.join(format!("job-{job}.lock"));
        if path.exists() {
            let lock = std::fs::OpenOptions::new()
                .read(true)
                .write(true)
                .open(path)
                .unwrap();
            lock.lock().unwrap();
        }
    }
    std::fs::remove_dir_all(root.parent().unwrap()).unwrap();
}

fn setup(worker: &str) -> kah_cli::observer_review::state::State {
    use kah_cli::{
        observer_review::state::{Pending, Purpose, State},
        relay_store::Endpoint,
    };
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
    let mut state = State::new(
        Endpoint {
            provider: "codex".into(),
            session: worker.into(),
            project: home.clone(),
            pane: None,
            socket: None,
        },
        &home,
    )
    .unwrap();
    state.pending = Some(Pending {
        id: "review-stall".into(),
        purpose: Purpose::Drift,
        turn: "turn".into(),
        head: None,
        created_ms: 0,
        native_tokens_before: None,
    });
    state
}

#[test]
fn delayed_observation_uses_record_write_time_instead_of_renewing_silence() {
    let mut observation = sample(0);
    observation.native_active = true;
    observation.transcript_version = Some((10, 0));
    let first = transition(None, &observation);
    observation.now_ms = 1_200_000;
    observation.transcript_version = Some((20, 1000));
    let quiet = transition(Some(&first), &observation);
    assert_eq!(quiet.last_progress_ms, 1000);
    assert!(
        quiet.stalled,
        "late watcher observes old records, not fresh progress"
    );
}

#[test]
fn acknowledged_report_handoff_never_stalls_or_redelivers_while_host_waits_for_lock() {
    use kah_cli::observer_review::{liveness, state::Store};
    let worker = format!("liveness-acked-{}", std::process::id());
    let state = setup(&worker);
    let held = Store::open(&worker).unwrap();
    held.save(&state).unwrap();
    std::fs::create_dir_all(state.observer.workspace.join("outbox")).unwrap();
    std::fs::write(
        state
            .observer
            .workspace
            .join("outbox/submitted-review-stall.json"),
        "{}",
    )
    .unwrap();
    kah_cli::unattended_report::save(
        &held.directory,
        "report-ready.json",
        &serde_json::json!({"request":"review-stall"}),
    )
    .unwrap();
    liveness::run(&worker).unwrap();
    let path = held.directory.join("liveness/progress.json");
    let mut prior: Progress = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    // A recent ACK hands work to the host; the separate host-stall regression
    // covers prolonged lack of actual result/decision progress after that ACK.
    prior.last_progress_ms = prior.sample.now_ms;
    std::fs::write(&path, serde_json::to_vec(&prior).unwrap()).unwrap();
    liveness::run(&worker).unwrap();
    let after: Progress = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    assert!(
        !after.stalled,
        "recent ACK transfers responsibility to host consumption"
    );
    assert!(liveness::notice(&worker).unwrap().is_none());
    assert!(!held.directory.join("redelivery.json").exists());
    drop(held);
    cleanup_jobs(&worker);
}
