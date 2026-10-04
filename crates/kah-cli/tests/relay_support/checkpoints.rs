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
    );
    let initial = Reviews::read(&worker).unwrap().unwrap();
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
        let answer =
            &first["judgment"]["answers"][format!("deferral.{}", item["id"].as_str().unwrap())];
        answer["choice"] == "necessary_future"
            && answer["confidence"].as_f64().unwrap()
                >= kah_observer::judge::questions::COMPLETE_CONFIDENCE
            && answer["probabilities"]["necessary_future"]
                .as_f64()
                .unwrap()
                >= kah_observer::judge::questions::COMPLETE_PROBABILITY
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
    panes.0[0].prompt(task).unwrap();
    super::linked_steer::wait_input(worker, task);
    ready(&panes.0[0], provider);
    let mut result = checkpoint(
        worker,
        provider,
        panes,
        Some(first),
        &files::git_oid(project, "refs/tags/phase-2"),
    );
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
    let reminder: Value = serde_json::from_slice(
        &fs::read(Reviews::directory(worker).unwrap().join("reminder.json")).unwrap(),
    )
    .unwrap();
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

fn repaired(
    worker: &str,
    provider: &str,
    project: &Path,
    panes: &mut Panes,
    second: &Value,
) -> Value {
    let task = "Continue by consuming the checkpoint feedback. Correct cumulative integration before Phase 3, run all three tests again after committing the correction, and stop. Do not change PLAN.md or tests or start Phase 3. Report the exact verified commit; the host user will request the next checkpoint with kah gate.";
    panes.0[0].prompt(task).unwrap();
    super::linked_steer::wait_input(worker, task);
    ready(&panes.0[0], provider);
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
    );
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
) -> Value {
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
                if checkpoint["head"] == head && previous.is_none_or(|p| p["request"] != request) {
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
        let gap = fs::read_to_string(Reviews::directory(worker).unwrap().join("gap.json"))
            .unwrap_or_default();
        assert!(gap.is_empty(), "checkpoint gap: {gap}");
        std::thread::sleep(Duration::from_millis(250));
    }
}
