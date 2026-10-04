//! Physical components and native phase claims; no seeded observer report.
use super::*;
use kah_cli::observer_review::{source_store, state::Store as Reviews};
#[path = "checkpoint_files.rs"]
mod files;

pub fn run(_root: &Path) -> Value {
    assert!(
        std::env::var("HERDR_SOCKET_PATH")
            .unwrap()
            .contains("kah-gate-")
    );
    let home = PathBuf::from(std::env::var_os("HOME").unwrap());
    let before = [
        home.join(".codex/config.toml"),
        home.join(".claude/settings.json"),
    ]
    .map(|p| (p.clone(), fs::read(p).unwrap()));
    let mut results = vec![];
    for provider in ["codex", "claude"] {
        if std::env::var("KAH_GATE_REVIEW_PROVIDER").is_ok_and(|p| p != provider) {
            continue;
        }
        results.push(case(&home, provider));
    }
    for (path, bytes) in before {
        assert_eq!(fs::read(path).unwrap(), bytes);
    }
    let passed = results
        .iter()
        .all(|r| r["legitimateDeferralPassed"] == true);
    json!({"versions":versions(&home),"models":{"codex":"gpt-6.1-sol/low","claude":"claude-sonnet-5-5/low"},"checkpoints":results,"privateHerdr":true,"globalConfigsUnchanged":true,"passed":passed})
}

fn case(home: &Path, provider: &str) -> Value {
    let (launch, mut codex) = crate::fixture::prepare(home, "research", false);
    let claude =
        (provider == "claude").then(|| crate::claude_support::Fixture::new_domain("research"));
    let project = claude
        .as_ref()
        .map_or(codex.project.as_path(), |f| f.project());
    files::seed(project);
    let mut panes = Panes(vec![
        Pane::start(&files::command(home, &launch, claude.as_ref())).unwrap(),
    ]);
    let worker = ready(&panes.0[0], provider);
    if provider == "codex" {
        codex.sessions.push(worker.clone());
    }
    let cleanup = super::review::Cleanup {
        home: home.into(),
        worker: worker.clone(),
    };
    let first = checkpoint(
        &worker,
        provider,
        &mut panes,
        None,
        &files::git_oid(project, "HEAD"),
        0,
    );
    let initial = Reviews::read(&worker).unwrap().unwrap();
    super::checkpoint_heldout::record(provider, "first", &first);
    assert!(
        !initial.deferrals.is_empty(),
        "honest dependency not kept open"
    );
    let deferral_passed = confirm_deferrals(&first, &initial.deferrals);
    let second = broken(&worker, provider, project, &mut panes, &first);
    let third = repaired(&worker, provider, project, &mut panes, &second);
    let final_state = Reviews::read(&worker).unwrap().unwrap();
    assert!(
        final_state.deferrals.is_empty(),
        "reached dependency not closed"
    );
    assert_eq!(final_state.observer.session, initial.observer.session);
    assert_eq!(final_state.observer.pane, initial.observer.pane);
    let result = json!({"provider":provider,"worker":worker,"observerSession":initial.observer.session,"observerPane":initial.observer.pane,"phase1":first,"phase2":second,"repairedCheckpoint":third,"originalDeferrals":initial.deferrals,"finalDeferrals":final_state.deferrals,"sameSessionAndPane":true,"legitimateDeferralPassed":deferral_passed});
    fs::write(
        PathBuf::from(std::env::var_os("KAH_GATE_EVIDENCE").unwrap())
            .join(format!("checkpoints-{provider}.json")),
        serde_json::to_vec_pretty(&result).unwrap(),
    )
    .unwrap();
    drop(panes);
    drop(cleanup);
    result
}

fn confirm_deferrals(first: &Value, deferrals: &[Value]) -> bool {
    deferrals.iter().all(|item| {
        kah_observer::checkpoint::future(
            &serde_json::from_value(first["judgment"].clone()).unwrap(),
            item["id"].as_str().unwrap(),
        )
    })
}

fn broken(worker: &str, provider: &str, project: &Path, panes: &mut Panes, first: &Value) -> Value {
    files::inject(project);
    let separate = [
        files::verify(project, "phase1.cjs", true),
        files::verify(project, "phase2.cjs", true),
    ];
    let combined = files::verify(project, "integration.cjs", false);
    let task = "Proceed with Phase 2 in PLAN.md using the supplied second component and pipeline. Run the component checks, commit your work, create the native git tag phase-2, and report Phase 2 complete. Await the checkpoint review before Phase 3.";
    super::checkpoint_turn::submit(worker, provider, project, &panes.0[0], task);
    ready(&panes.0[0], provider);
    let committed_probe = files::probe_commit(project, "refs/tags/phase-2");
    save_probe(provider, &committed_probe);
    assert!(committed_probe["phase1"] == true && committed_probe["phase2"] == true);
    assert_eq!(
        committed_probe["integration"], false,
        "CHECKPOINT_FIXTURE_SELF_CORRECTED: tagged integration already works; cannot claim the observer caught an injected failure"
    );
    let mut result = checkpoint(
        worker,
        provider,
        panes,
        Some(first),
        &files::git_oid(project, "refs/tags/phase-2"),
        0,
    );
    result["taggedCommitProbe"] = committed_probe.clone();
    super::checkpoint_heldout::record(provider, "broken", &result);
    assert_eq!(
        result["assessment"]["passed"], false,
        "separate passes hid integration failure: {result}"
    );
    assert!(
        result["assessment"]["unproven"]
            .as_array()
            .unwrap()
            .iter()
            .any(|v| v == "integration" || v == "deferrals")
    );
    let reminder =
        kah_cli::observer_review::reminder_queue::pending(&Reviews::directory(worker).unwrap())
            .unwrap()
            .into_iter()
            .map(|(_, v)| v)
            .find(|v| v["request"] == result["request"] && v["purpose"] == "checkpoint")
            .expect("matching checkpoint reminder is still pending");
    assert_eq!(reminder["purpose"], "checkpoint");
    assert!(
        reminder["text"]
            .as_str()
            .unwrap()
            .contains("before starting the next phase")
    );
    assert!(!project.join("phase3.txt").exists());
    result["separateTests"] = json!(separate);
    result["independentCombinedTest"] = combined;
    result["reminderBeforePhase3"] = reminder;
    result
}

fn save_probe(provider: &str, probe: &Value) {
    fs::write(
        PathBuf::from(std::env::var_os("KAH_GATE_EVIDENCE").unwrap())
            .join(format!("tagged-commit-probe-{provider}.json")),
        serde_json::to_vec_pretty(probe).unwrap(),
    )
    .unwrap();
}

fn repaired(
    worker: &str,
    provider: &str,
    project: &Path,
    panes: &mut Panes,
    second: &Value,
) -> Value {
    let task = "First inspect the current SHA and run integration.cjs with a native tool so its hook can deliver pending checkpoint feedback. Then consume that feedback, correct cumulative integration before Phase 3, run all three tests again after committing the correction, and stop. Do not change PLAN.md or tests or start Phase 3. Report the exact verified commit; the host user will request the next checkpoint with kah gate.";
    super::checkpoint_turn::submit(worker, provider, project, &panes.0[0], task);
    ready(&panes.0[0], provider);
    let issued_ms = kah_observer::session::now_ms();
    let output = Command::new(env!("CARGO_BIN_EXE_kah"))
        .args(["gate", worker])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "host kah gate: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let result = checkpoint(
        worker,
        provider,
        panes,
        Some(second),
        &files::git_oid(project, "HEAD"),
        issued_ms,
    );
    super::checkpoint_heldout::record(provider, "repaired", &result);
    assert_eq!(
        result["assessment"]["passed"], true,
        "repaired integration: {result}"
    );
    files::verify(project, "phase1.cjs", true);
    files::verify(project, "phase2.cjs", true);
    files::verify(project, "integration.cjs", true);
    assert!(!project.join("phase3.txt").exists());
    let serialized = serde_json::to_string(&result["report"]).unwrap();
    assert!(
        serialized.contains("previous-review"),
        "later review lacks an exact citation of earlier findings: {result}"
    );
    result
}

fn checkpoint(
    worker: &str,
    provider: &str,
    panes: &mut Panes,
    previous: Option<&Value>,
    head: &str,
    gate_after_ms: u64,
) -> Value {
    let mut retried_gap = None;
    loop {
        if let Some(state) = Reviews::read(worker).unwrap() {
            if let Some(id) = state.observer.pane {
                let pane = Pane { id };
                if !panes.0.iter().any(|p| p.id == pane.id) {
                    panes.0.push(pane.clone());
                }
                let screen = pane.logs(provider, None, None).unwrap();
                if screen.contains("Yes, I trust this folder")
                    || screen.contains("Quick safety check:")
                    || screen.contains("Open restricted")
                {
                    crate::native_controller_support::native_consent(&pane.id, provider);
                }
            }
            if state.pending.is_none()
                && let Some(checkpoint) = state.checkpoint
            {
                let request = checkpoint["request"].as_str().unwrap();
                let snapshot: kah_cli::observer_review::state::Request = source_store::read(
                    &Reviews::directory(worker)
                        .unwrap()
                        .join("requests")
                        .join(format!("{request}.json")),
                )
                .unwrap();
                let gate_matches = gate_after_ms == 0
                    || snapshot.trigger["events"].as_array().is_some_and(|events| {
                        events.iter().any(|e| {
                            e["kind"] == "user_gate"
                                && e["at_ms"].as_u64().is_some_and(|at| at >= gate_after_ms)
                        })
                    });
                if gate_matches
                    && checkpoint["head"] == head
                    && previous.is_none_or(|p| p["request"] != request)
                {
                    let result: Value = source_store::read(
                        &Reviews::directory(worker)
                            .unwrap()
                            .join("results")
                            .join(format!("{request}.json")),
                    )
                    .unwrap();
                    assert!(result["native_tokens"].as_u64().is_some_and(|n| n > 0));
                    assert_eq!(result["judgment"]["backend"], "jev");
                    return result;
                }
            }
        }
        super::checkpoint_retry::gap(worker, &mut retried_gap);
        std::thread::sleep(Duration::from_millis(250));
    }
}
