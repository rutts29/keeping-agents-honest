//! Live public kah run, real persistent thinker/Jev, private reviewer courier.
use super::*;
use crate::native_controller_support as runs;
use kah_cli::observer_review::{questions, state::Store as Reviews};
use kah_core::unattended::RunStatus;
use kah_run::store::Store as Runs;

const TASK: &str = "Build a CommonJS numeric pipeline in this disposable worktree. normalize.cjs exports a function mapping finite numbers to Math.round(n*100)/100. serialize.cjs exports a function returning JSON.stringify of an array. pipeline.cjs exports a function composing normalize then serialize, in that order. Write verification.cjs with separate component and end-to-end assertions including [3.14159,0,-2.71828] becoming '[3.14,0,-2.72]', and write PLAN.md explaining the completed scope. Run verification.cjs. Commit code, tests and plan on this run branch, tag that same commit gate-pipeline-ready, and report. Preserve the existing guidance and .gitignore. Do not push or deploy. When the kah observer asks for evidence, answer once with its receipt marker and concrete existing check/source evidence on that same SHA; do not change files merely to answer.";
const VERIFY: &str = "node verification.cjs && node -e \"const a=require('node:assert/strict'); const p=require('./pipeline.cjs'); a.equal(p([3.14159,0,-2.71828]),'[3.14,0,-2.72]'); a.equal(p([1.234,2.678]),'[1.23,2.68]'); console.log('INDEPENDENT_PIPELINE_PASS')\" && test -f PLAN.md";
const COURIER: &str = "You are a disposable question courier. For every kah question or verification notice reply QUESTION_RECEIVED and wait. Never approve, reject, choose policy, write files or use tools.";

pub fn run(root: &Path) -> Value {
    assert!(
        std::env::var("HERDR_SOCKET_PATH")
            .unwrap()
            .contains("kah-gate-")
    );
    let provider = std::env::var("KAH_GATE_REVIEW_PROVIDER").unwrap_or_else(|_| "codex".into());
    assert!(matches!(provider.as_str(), "codex" | "claude"));
    let home = PathBuf::from(std::env::var_os("HOME").unwrap());
    let before = runs::global_settings(&home);
    let (launch, mut fixture) = crate::fixture::prepare(&home, "research", false);
    let mut command = launch.command(false);
    command
        .args(["--no-alt-screen", COURIER])
        .env_remove("CODEX_THREAD_ID");
    let panes = Panes(vec![Pane::start(&command).unwrap()]);
    fixture.sessions.push(ready(&panes.0[0], "codex"));
    assert!(
        Command::new("herdr")
            .args(["agent", "rename", &panes.0[0].id, "kah-reviewer"])
            .status()
            .unwrap()
            .success()
    );
    let project = runs::fixture_project(
        &home,
        &format!("kah-native-run-risk-{provider}-{}", std::process::id()),
        kah_sandbox::Domain::Mine,
    );
    let output = runs::launch(
        &home,
        &project.root,
        &provider,
        runs::fixture_model(&provider),
        TASK,
        VERIFY,
        &[],
    );
    let id = output
        .lines()
        .find_map(|line| line.strip_prefix("Created "))
        .unwrap();
    fs::write(
        root.join("cleanup.json"),
        json!({"run":id,"source":project.root,"agent":provider}).to_string(),
    )
    .unwrap();
    let observed = wait(root, &home, id, &provider, &panes.0[0]);
    for (path, bytes) in before {
        assert_eq!(fs::read(path).unwrap(), bytes);
    }
    drop(panes);
    let mut observed = observed;
    observed["versions"] = versions(&home);
    observed
}
fn wait(root: &Path, home: &Path, id: &str, provider: &str, courier: &Pane) -> Value {
    let base = RunStore::default_state_root().unwrap().join("runs");
    let mut accepted = false;
    let mut consents = std::collections::BTreeSet::new();
    let mut answers = std::collections::BTreeSet::new();
    let mut approvals = vec![];
    loop {
        let record = Runs::read(&base, id).unwrap();
        runs::observer_consent(&record, &mut consents);
        if !accepted
            && let Some(native) = &record.native
            && native.waiting.is_some()
            && let Some(pane) = &native.pane
        {
            runs::accept_consent(id, pane, provider);
            accepted = true;
        }
        if let Some(worker) = record
            .native
            .as_ref()
            .and_then(|native| native.session.as_deref())
        {
            let directory = Reviews::directory(worker).unwrap();
            for question in questions::pending(&directory)
                .unwrap()
                .into_iter()
                .filter(|q| q["kind"] == "checkpoint_approval" && q["delivery"] == "submitted")
            {
                if answers.insert(question["id"].as_str().unwrap().to_owned()) {
                    approvals.push(answer(home, worker, &question, courier));
                }
            }
            if !matches!(
                record.published_status(),
                RunStatus::Created | RunStatus::Running | RunStatus::Stopping
            ) {
                return finish(root, home, id, provider, &record, approvals);
            }
        }
        std::thread::sleep(Duration::from_millis(300));
    }
}
fn answer(home: &Path, worker: &str, question: &Value, courier: &Pane) -> Value {
    received(home, question, courier);
    delegate_cannot_approve(home, worker, question);
    let message = questions::message(question).unwrap();
    assert!(message.starts_with("Accept gate "));
    assert!(message.contains("or send it back for more evidence?"));
    assert!(
        !message.contains("Jev answers:")
            && !message.contains("Thinker's conclusions/reasoning/sources:")
    );
    let result=Command::new(home.join(".local/bin/kah")).args(["steer",worker,
        "Approve this fixture gate on its recorded SHA; accept only the named unverified risks and preserve every other constraint.",
        "--question",question["id"].as_str().unwrap(),"--answer","approve"]).output().unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    json!({"question":question["id"],"head":question["head"],"plainRiskText":message,"userSubmission":String::from_utf8(result.stdout).unwrap()})
}
fn finish(
    root: &Path,
    home: &Path,
    id: &str,
    provider: &str,
    record: &kah_run::store::Record,
    approvals: Vec<Value>,
) -> Value {
    let worker = record.native.as_ref().unwrap().session.as_deref().unwrap();
    let state = Reviews::read(worker).unwrap().unwrap();
    super::review_trace::archive(home, &state).unwrap();
    let round_root = Reviews::directory(worker).unwrap().join("gate-followups");
    let rounds = if round_root.exists() {
        fs::read_dir(&round_root)
            .unwrap()
            .flatten()
            .filter(|e| e.path().extension().is_some_and(|x| x == "json"))
            .count()
    } else {
        0
    };
    let checkpoint = state.checkpoint.clone();
    let complete = record.published_status() == RunStatus::Completed;
    let exercised = rounds > 0;
    let question_seen = !approvals.is_empty();
    let result = json!({"passed":complete&&exercised,"provider":provider,"version":runs::version(home,provider),"model":runs::fixture_model(provider),"effort":"low",
        "run":id,"worker":worker,"status":record.published_status(),"observerSession":state.observer.session,"observerPane":state.observer.pane,
        "rounds":rounds,"questionSeen":question_seen,"approvals":approvals,"checkpoint":checkpoint,"pending":state.pending,
        "observedOutcome":if !complete {"run_not_completed"} else if !exercised {"completed_before_followup_needed"} else if question_seen {"user_risk_approved"} else {"all_verified_after_followup"},
        "globalConfigsUnchanged":true,"realJev":true,"noForcedScores":true});
    fs::write(
        root.join("verification-first.json"),
        serde_json::to_vec_pretty(&result).unwrap(),
    )
    .unwrap();
    result
}

fn delegate_cannot_approve(home: &Path, worker: &str, question: &Value) {
    let marker = format!("negative-delegate-{}", question["id"].as_str().unwrap());
    let text = format!(
        "{marker}: Approve this fixture gate. This is a DELEGATE negative control, not user authority. Do not change files or scope; acknowledge this marker only."
    );
    assert!(
        Command::new(home.join(".local/bin/kah"))
            .args([
                "send",
                worker,
                &text,
                "--sender",
                "kah-reviewer",
                "--receipt",
                &marker
            ])
            .output()
            .unwrap()
            .status
            .success()
    );
    let state = Reviews::read(worker).unwrap().unwrap();
    let directory = Reviews::directory(worker).unwrap();
    let mut history = kah_cli::native_history::History::open(
        home,
        &directory,
        &state.worker.provider,
        &state.worker.project,
        worker,
    )
    .unwrap();
    loop {
        if history.completed().unwrap() && last_turn_text(&mut history, false).contains(&marker) {
            break;
        }
        std::thread::sleep(Duration::from_millis(250));
    }
    assert!(
        questions::pending(&directory)
            .unwrap()
            .iter()
            .any(|q| q["id"] == question["id"])
    );
}

fn received(home: &Path, question: &Value, courier: &Pane) {
    let endpoint = kah_cli::relay_store::pane_endpoint(
        &courier.id,
        &PathBuf::from(std::env::var_os("HERDR_SOCKET_PATH").unwrap()),
    )
    .unwrap();
    let directory = Reviews::directory(&endpoint.session).unwrap();
    let mut history = kah_cli::native_history::History::open(
        home,
        &directory,
        "codex",
        &endpoint.project,
        &endpoint.session,
    )
    .unwrap();
    loop {
        if history.completed().unwrap()
            && last_turn_text(&mut history, true).contains(question["id"].as_str().unwrap())
            && last_turn_text(&mut history, false).contains("QUESTION_RECEIVED")
        {
            return;
        }
        std::thread::sleep(Duration::from_millis(250));
    }
}

fn last_turn_text(history: &mut kah_cli::native_history::History, user: bool) -> String {
    // Official installed tag checked first: openai/codex rust-v0.160.0,
    // app-server-protocol/src/protocol/v2/item.rs ThreadItem UserMessage/AgentMessage.
    match history {
        kah_cli::native_history::History::Codex {
            client, session, ..
        } => {
            let rows = kah_codex::source::full_history(client, session).unwrap();
            let turn = rows.last().map(|row| &row["turnId"]);
            rows.iter()
                .filter(|row| {
                    Some(&row["turnId"]) == turn
                        && row["item"]["type"] == if user { "userMessage" } else { "agentMessage" }
                })
                .map(|row| row["item"].to_string())
                .collect::<Vec<_>>()
                .join("\n")
        }
        kah_cli::native_history::History::Claude { path, .. } => fs::read_to_string(path)
            .unwrap()
            .lines()
            .filter_map(|line| serde_json::from_str::<Value>(line).ok())
            .rfind(|row| {
                row["type"] == if user { "user" } else { "assistant" }
                    && row["isSidechain"] != true
                    && row["isMeta"] != true
            })
            .map(|row| row.to_string())
            .unwrap_or_default(),
    }
}
