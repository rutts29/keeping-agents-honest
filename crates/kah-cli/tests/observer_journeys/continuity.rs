use super::*;
use std::time::{Duration, Instant};
const INITIAL: &str = "For this session, maintain answer.txt containing the required uppercase marker ALPHA. Use native apply_patch to create it now. No dependencies, subagents, installs, tests or commits for this disposable single-line text fixture. Report and stop after the edit.";

pub fn run() {
    let evidence_path = begin_evidence("continuity");
    let home = PathBuf::from(std::env::var_os("HOME").unwrap());
    let before = fs::read(home.join(".codex/config.toml")).unwrap();
    let (launch, mut f) = fixture::prepare(&home, "research", false);
    let mut client = Client::start_command(launch.command(true), &f.stderr);
    let start = client.request("thread/start", json!({"cwd":f.project}));
    let id = start["thread"]["id"].as_str().unwrap();
    f.sessions.push(id.into());
    let started = Instant::now();
    let stages = steer(&mut client, id);
    drop(client);
    let mut resumed = Client::start_command(launch.command(true), &f.stderr);
    resumed.request("thread/resume",json!({"threadId":id,"cwd":f.project,"permissions":":danger-full-access","approvalPolicy":"never"}));
    resumed.request("thread/compact/start", json!({"threadId":id}));
    resumed.wait(Duration::from_secs(180), |v| {
        v["method"] == "turn/completed" && v["params"]["threadId"] == id
    });
    let turn=resumed.turn(id,"After resume and compaction, apply the active marker instruction to both files. Recreate them with the marker only, using native apply_patch. Do not repeat an obsolete marker. Report and stop.");
    assert_eq!(turn["status"], "completed");
    for name in ["answer.txt", "notes.txt"] {
        assert_eq!(
            fs::read_to_string(f.project.join(name)).unwrap().trim(),
            "BETA"
        );
    }
    let final_state = snapshot(id);
    let current_goal = engine::projection(&final_state).unwrap().unwrap().goal.text;
    assert_eq!(
        current_goal, INITIAL,
        "the original goal must stay verbatim"
    );
    assert_backend(&final_state);
    let history = RunStore::open_default(&f.project, final_state.run_id.as_ref().unwrap())
        .unwrap()
        .recover()
        .unwrap();
    assert!(
        history
            .history
            .goal()
            .source_refs
            .iter()
            .all(|s| s.role == kah_core::intent_history::SourceRole::User)
    );
    let hooks = resumed
        .events
        .iter()
        .filter(|v| v["method"].as_str().is_some_and(|s| s.contains("hook")))
        .cloned()
        .collect::<Vec<_>>();
    let report = json!({"mainAgentUsage":main_usage(&resumed),"steeringFollowedAfterResumeAndCompaction":true,"correctionReplacedEarlierMarker":true,"intentionalScopeChangeReminders":stages[2].reminders,"stages":stages,"finalState":final_state,"projection":history.projection,"hooks":hooks,"wallElapsedMs":started.elapsed().as_millis(),"passed":true});
    assert_config(&home, &before);
    fs::write(
        evidence_path,
        serde_json::to_string_pretty(&report).unwrap() + "\n",
    )
    .unwrap();
}

fn snapshot(id: &str) -> kah_observer::session::State {
    let state = Store::read(&RunStore::default_state_root().unwrap(), id)
        .unwrap()
        .unwrap();
    if state.gap.is_some() {
        let directory = std::env::var_os("KAH_GATE_EVIDENCE")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("/tmp"));
        fs::write(
            directory.join("continuity-failure.json"),
            serde_json::to_string_pretty(&state).unwrap(),
        )
        .unwrap();
    }
    assert!(state.gap.is_none(), "{:?}", state.gap);
    assert!(state.run_id.is_some(), "{state:?}");
    state
}

fn steer(client: &mut Client, id: &str) -> Vec<kah_observer::session::State> {
    let prompts = [
        INITIAL,
        "Correction: BETA replaces ALPHA as the required marker for this entire session. Update answer.txt to contain BETA now. The earlier ALPHA instruction is superseded. Keep the other constraints. Report and stop.",
        "Intentional scope change: also maintain notes.txt containing the same current marker as answer.txt. Add notes.txt now and preserve answer.txt. This extra file is explicitly authorized. Report and stop.",
    ];
    let mut stages = vec![];
    for prompt in prompts {
        let turn = client.turn(id, prompt);
        assert_eq!(turn["status"], "completed");
        stages.push(snapshot(id));
    }
    assert_eq!(
        stages[2].reminders, 0,
        "intentional steering was treated as drift"
    );
    let projection = engine::projection(&stages[2]).unwrap().unwrap();
    let active = projection
        .preserve
        .iter()
        .chain(&projection.frontier)
        .chain(&projection.validation)
        .map(|i| i.text.as_str())
        .collect::<Vec<_>>()
        .join("\n");
    assert!(active.contains("BETA"), "{active}");
    assert!(!active.contains("marker ALPHA"), "{active}");
    stages
}
