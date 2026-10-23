//! Jev refused a worker reply too large for its context. The claim
//! file stayed, so every later Stop failed on the same file and the
//! delivered-change check never ran. A fake Jev transport; no real Jev call.
use kah_cli::observer_review::{
    claims,
    state::{State, Store},
};
use kah_observer::{
    judge::{self, Answer, Judgment},
    session::{self, Binding},
};
use serde_json::Value;
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    process::Command,
    time::Duration,
};

/// A synthetic worker reply for the oversized-input regression.
const REPLY: &str = "The example transform is implemented on branch `kah/run-example`.\n\nThe fixture checks passed.\n\nHEAD is `aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa`. The working tree is clean; no remote action was taken.";
/// The fake Jev refuses a state larger than this; the reply is 2 MiB.
const LIMIT: usize = 100_000;
const LARGE: usize = 2 << 20;

#[test]
fn a_too_large_reply_goes_to_the_observer_and_never_blocks_later_stops() {
    let root = std::env::temp_dir().join(format!("kah-claim-oversized-{}", std::process::id()));
    fs::create_dir_all(&root).unwrap();
    let output = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "oversized_claim_child", "--nocapture"])
        .env("KAH_OVERSIZED_CLAIM_FIXTURE", &root)
        .env("HOME", &root)
        .env("XDG_STATE_HOME", root.join("state"))
        .output()
        .unwrap();
    fs::remove_dir_all(&root).unwrap();
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn oversized_claim_child() {
    let Some(root) = std::env::var_os("KAH_OVERSIZED_CLAIM_FIXTURE") else {
        return;
    };
    let root = PathBuf::from(root);
    let large = REPLY.repeat(LARGE / REPLY.len() + 1);
    // The observer judges the claim: a doubtful claim event with the whole reply.
    let head = worker(&root, "claims-handoff", true);
    stop(&root, "claims-handoff", "turn-1", &large);
    let done = records("claims-handoff", "claim-events", "done");
    assert_eq!(done.len(), 1);
    assert_eq!(done[0]["jev_refusal"]["known_oversized"], true);
    assert_eq!(done[0]["source"]["native_assistant_text"], large.as_str());
    let events = records("claims-handoff", "checkpoint-events", "json");
    assert_eq!(events.len(), 1, "the claim was not handed to the observer");
    assert_eq!(events[0]["kind"], "native_phase_claim");
    assert_eq!(events[0]["uncertain"], true);
    assert_eq!(events[0]["review_class"], "commit");
    assert_eq!(events[0]["head"], head.as_str());
    assert_eq!(events[0]["source"]["native_assistant_text"], large.as_str());
    // A later Stop is screened as usual.
    stop(&root, "claims-handoff", "turn-2", REPLY);
    assert_eq!(records("claims-handoff", "claim-events", "done").len(), 2);
    // The hand-off also fails (Git metadata unreadable): a visible gap.
    worker(&root, "claims-gap", false);
    stop(&root, "claims-gap", "turn-1", &large);
    let done = records("claims-gap", "claim-events", "done");
    assert_eq!(done.len(), 1);
    assert_eq!(done[0]["gap"]["known_oversized"], true);
    assert!(
        done[0]["gap"]["handoff_error"].is_string(),
        "{}",
        done[0]["gap"]
    );
    assert!(records("claims-gap", "checkpoint-events", "json").is_empty());
}

/// The worker's Stop and its claims job pass, as claims::run: the screen, then
/// delivered::schedule. The claim file must be gone after the pass.
fn stop(home: &Path, worker: &str, turn: &str, text: &str) {
    claims::schedule(worker, turn, text).unwrap();
    let screened = claims::screen(worker, home, fake_jev).map_err(|e| e.to_string());
    let left = records(worker, "claim-events", "json").len();
    println!("{worker} {turn}: screen {screened:?}, claim files left {left}");
    assert_eq!(screened, Ok(()), "delivered::schedule does not run");
    assert_eq!(left, 0, "the claim file stays and blocks later Stops");
}

fn fake_jev(_: &Binding, input: &Value) -> Result<Judgment, judge::Error> {
    if input.to_string().len() > LIMIT {
        let mut error = judge::Error::new("JEV_CONTEXT_LIMIT");
        error.stage = "judge_http";
        error.status = Some(400);
        return Err(error);
    }
    let answers = ["phase_claim", "implementation_claim"]
        .map(|q| (q.to_owned(), Answer::Noul { noul: 0.02 }));
    Ok(Judgment {
        backend: "jev".into(),
        model: "jev-1.13.0".into(),
        answers: BTreeMap::from(answers),
        metrics: Default::default(),
        fallback_reason: None,
    })
}

fn records(worker: &str, directory: &str, extension: &str) -> Vec<Value> {
    let Ok(entries) = fs::read_dir(Store::directory(worker).unwrap().join(directory)) else {
        return vec![];
    };
    entries
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|e| e == extension))
        .map(|p| serde_json::from_slice(&fs::read(p).unwrap()).unwrap())
        .collect()
}

/// A worker whose project is under ~/repos/mine, so the head is host Git
/// metadata. Without `git` the project has a broken `.git`. Returns HEAD.
fn worker(home: &Path, name: &str, git: bool) -> String {
    let project = home.join("repos/mine").join(name);
    fs::create_dir_all(&project).unwrap();
    let git_at = |args: &[&str]| {
        let output = Command::new("git")
            .args(["-C", project.to_str().unwrap(), "-c", "user.name=kah"])
            .args([
                "-c",
                "user.email=kah@example.invalid",
                "-c",
                "commit.gpgsign=false",
            ])
            .args(args)
            .output()
            .unwrap();
        assert!(output.status.success(), "{output:?}");
        String::from_utf8(output.stdout).unwrap().trim().to_owned()
    };
    let head = if git {
        git_at(&["init", "-q"]);
        git_at(&["commit", "-q", "--allow-empty", "-m", "start"]);
        git_at(&["rev-parse", "HEAD"])
    } else {
        fs::write(project.join(".git"), "").unwrap();
        String::new()
    };
    let base = kah_core::run_store::RunStore::default_state_root().unwrap();
    let binding = Binding {
        session: name.into(),
        project: project.clone(),
        codex_home: home.join("codex"),
        native: "/nonexistent/native".into(),
        model: "claude-sonnet-5-5".into(),
        effort: "low".into(),
        timeout_secs: 90,
        judge: Default::default(),
        jev_model: "jev-latest".into(),
    };
    let mut primary = session::Store::open(&base, name, Duration::ZERO).unwrap();
    primary.save(&session::State::new(binding)).unwrap();
    drop(primary);
    let endpoint = kah_cli::relay_store::Endpoint {
        provider: "claude".into(),
        session: name.into(),
        project,
        pane: Some("w0:worker".into()),
        socket: Some(home.join("herdr.sock")),
    };
    let state = State::new(endpoint, home).unwrap();
    Store::open(name).unwrap().save(&state).unwrap();
    head
}
