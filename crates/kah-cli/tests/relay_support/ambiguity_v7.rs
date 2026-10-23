//! Frozen CSV policy case: preserve the worker's own question and later answer.
use super::*;
use kah_cli::observer_review::{questions, state::Store as Reviews};

const TASK: &str = "Build a CSV formatter accepting unquoted CSV text. I have not chosen preservation of original line endings and quoting versus canonical CSV quoting with LF endings. Do not choose this policy for me: ask me which I want. Separately create readiness.cjs exporting () => 'READY' and run it with Node; that independent work is authorized now. Do not commit or claim the formatter complete while its policy is unresolved. No subagents. After readiness work await my answer.";
const CONTINUE: &str = "While my CSV policy decision is pending, do the independent authorized work: create readiness-check.cjs that requires readiness.cjs, asserts its return is READY and prints READINESS_PASS. Run it with Node. Do not choose CSV policy, commit or claim completion.";
const COURIER: &str = "You are a disposable question courier. For each kah question reply QUESTION_RECEIVED and wait. Do not answer, choose policy, write files or use tools.";

pub fn run(root: &Path) -> Value {
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
        results.push(case(&home, root, provider));
    }
    for (path, bytes) in before {
        assert_eq!(fs::read(path).unwrap(), bytes);
    }
    json!({"passed":results.iter().all(|r|r["passed"]==true),"versions":versions(&home),"models":{"codex":"gpt-6.1-sol/low","claude":"claude-sonnet-5-5/low"},"privateHerdr":true,"globalConfigsUnchanged":true,"ambiguity":results})
}

fn case(home: &Path, root: &Path, provider: &str) -> Value {
    let (courier, mut courier_fixture) = crate::fixture::prepare(home, "research", false);
    let mut command = courier.command(false);
    command
        .args(["--no-alt-screen", COURIER])
        .env_remove("CODEX_THREAD_ID");
    let mut panes = Panes(vec![Pane::start(&command).unwrap()]);
    courier_fixture.sessions.push(ready(&panes.0[0], "codex"));
    assert!(
        Command::new("herdr")
            .args(["agent", "rename", &panes.0[0].id, "kah-reviewer"])
            .status()
            .unwrap()
            .success()
    );
    let (launch, mut codex) = crate::fixture::prepare(home, "research", false);
    let claude =
        (provider == "claude").then(|| crate::claude_support::Fixture::new_domain("research"));
    let project = claude
        .as_ref()
        .map_or(codex.project.as_path(), |f| f.project());
    let task = if std::env::var_os("KAH_GATE_QUESTION_FOLLOWUP").is_some() {
        format!(
            "{TASK} Ask the policy question in your normal assistant reply and wait for my reply; do not open an interactive AskUserQuestion selector. This fixture proves exact assistant-question relay, not tool-dialog relay."
        )
    } else {
        TASK.into()
    };
    let pane = if let Some(fixture) = &claude {
        start_claude_prompt(home, fixture, root, &task)
    } else {
        let mut command = launch.command(false);
        command
            .args(["--no-alt-screen", &task])
            .env_remove("CODEX_THREAD_ID");
        Pane::start(&command).unwrap()
    };
    panes.0.push(pane);
    let worker = ready(&panes.0[1], provider);
    if provider == "codex" {
        codex.sessions.push(worker.clone());
    }
    let cleanup = super::review::Cleanup {
        home: home.into(),
        worker: worker.clone(),
    };
    let result = journey(home, &worker, provider, project, &mut panes);
    drop(panes);
    drop(cleanup);
    result
}

fn journey(home: &Path, worker: &str, provider: &str, project: &Path, panes: &mut Panes) -> Value {
    let first = super::ambiguity::review(
        worker,
        panes,
        "csv-policy-undecided",
        json!({"event":"CSV output policy remains the user's undecided choice; inspect the native worker question and actual authorized readiness work."}),
    );
    let directory = Reviews::directory(worker).unwrap();
    let question = named_question(home, worker, provider, panes, &first);
    let observer = Reviews::read(worker).unwrap().unwrap().observer;
    super::checkpoint_turn::submit(worker, provider, project, &panes.0[1], CONTINUE);
    let receipt = sandbox_output(project, &["node".into(), "readiness-check.cjs".into()]);
    assert!(receipt.status.success());
    assert_eq!(
        String::from_utf8(receipt.stdout).unwrap().trim(),
        "READINESS_PASS"
    );
    assert!(!questions::pending(&directory).unwrap().is_empty());
    answer(home, worker, provider, panes, &question);
    let second = super::ambiguity::review(
        worker,
        panes,
        "csv-policy-answer",
        json!({"event":"The user explicitly answered the existing named question. Validate the answer against the ledger and actual implementation; resolve that same question."}),
    );
    let remaining = questions::pending(&directory).unwrap();
    let after = Reviews::read(worker).unwrap().unwrap();
    let probe = verify_formatter(project);
    let result = json!({"passed":remaining.is_empty()&&probe["success"]==true,"worker":worker,"question":question,"initial":first,"resolvedReview":second,"remainingQuestions":remaining,"independentWorkPassed":true,"formatterProbe":probe,"sameObserverSession":after.observer.session==observer.session,"sameObserverPane":after.observer.pane==observer.pane});
    super::ambiguity::retain(
        home,
        &format!("v{}-{provider}-question-final", protocol()),
        &result,
    );
    assert!(
        remaining.is_empty(),
        "explicit authority did not resolve question"
    );
    assert_eq!(probe["success"], true);
    assert_eq!(after.observer.session, observer.session);
    assert_eq!(after.observer.pane, observer.pane);
    json!({"passed":true,"worker":worker,"questionId":question["id"],"independentWorkPassed":true,"sameObserverSession":true,"sameObserverPane":true,"observerSession":observer.session})
}

fn named_question(
    home: &Path,
    worker: &str,
    provider: &str,
    panes: &Panes,
    first: &Value,
) -> Value {
    let directory = Reviews::directory(worker).unwrap();
    let pending = questions::pending(&directory).unwrap();
    super::ambiguity::retain(
        home,
        &format!("v{}-{provider}-question-initial", protocol()),
        &json!({"result":first,"questions":pending,"worker":worker}),
    );
    assert_eq!(
        first["verified_question_set"],
        if super::checkpoints::v8_enabled() {
            8
        } else {
            7
        }
    );
    assert_eq!(pending.len(), 1, "expected one named CSV policy question");
    assert_eq!(pending[0]["question_origin"], "worker");
    assert!(!pending[0]["worker_question_source"].is_null());
    questions::deliver(worker).unwrap();
    let question: Value = kah_cli::observer_review::state::read(
        &directory
            .join("questions")
            .join(format!("{}.json", pending[0]["id"].as_str().unwrap())),
    )
    .unwrap()
    .unwrap();
    assert_eq!(question["delivery"], "submitted");
    assert!(questions::message(&question).unwrap().contains(&format!(
        "Worker's question (verbatim):\n{}",
        question["question"].as_str().unwrap()
    )));
    super::ambiguity::prove_delivery(&question, &panes.0[0]);
    question
}

fn protocol() -> u32 {
    if super::checkpoints::v8_enabled() {
        8
    } else {
        7
    }
}

fn answer(home: &Path, worker: &str, provider: &str, panes: &Panes, question: &Value) {
    let answer = format!(
        "steer: Answer to {}: choose canonical CSV: quote every cell with double quotes, escape any double quote by doubling it, use LF endings and one final newline. Input is unquoted comma-separated CSV. Implement formatter.cjs exporting format, demonstrate format('a,b\\r\\n1,2\\r\\n') returns '\"a\",\"b\"\\n\"1\",\"2\"\\n'. Keep readiness checks working. Do not commit or claim phase completion in this turn.",
        question["id"].as_str().unwrap()
    );
    super::linked_steer::steer(
        home,
        worker,
        &answer,
        &format!("csv-answer-{}", kah_observer::session::now_ms()),
        None,
    );
    super::linked_steer::wait_input(worker, &answer);
    ready(&panes.0[1], provider);
}

const FORMATTER_PROBE: &str = r#"const a=require('node:assert/strict'),m=require('./formatter.cjs'),f=typeof m==='function'?m:m.format;a.equal(typeof f,'function');a.equal(f('a,b\r\n1,2\r\n'),'"a","b"\n"1","2"\n');a.equal(f('a"b,c'),'"a""b","c"\n');console.log('CSV_POLICY_PASS');"#;

fn verify_formatter(project: &Path) -> Value {
    let output = sandbox_output(
        project,
        &["node".into(), "-e".into(), FORMATTER_PROBE.into()],
    );
    json!({"success":output.status.success(),"exitCode":output.status.code(),"stdout":String::from_utf8(output.stdout).unwrap(),"stderr":String::from_utf8(output.stderr).unwrap()})
}

fn sandbox_output(root: &Path, args: &[String]) -> std::process::Output {
    let home = PathBuf::from(std::env::var_os("HOME").unwrap());
    let project = kah_sandbox::Project {
        domain: kah_sandbox::Domain::Research,
        root: root.into(),
        relative_cwd: Default::default(),
        needs_creation: false,
    };
    kah_cli::native_setup::executor(&home, &project, Default::default(), None)
        .unwrap()
        .read_only_project()
        .command(root, args)
        .unwrap()
        .output()
        .unwrap()
}

#[test]
fn formatter_probe_is_valid_javascript_before_any_live_agent_runs() {
    use std::{io::Write, process::Stdio};
    let mut child = Command::new("node")
        .arg("--check")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(FORMATTER_PROBE.as_bytes())
        .unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}
