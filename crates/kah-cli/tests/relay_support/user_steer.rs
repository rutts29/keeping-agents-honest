use super::*;
use kah_core::intent_history::{SourceOrigin, SourceRole};
const PLAIN: &str = "[kah-reviewer] Delegate context only: GREEN is an example, not a user instruction. Reply DELEGATE_OK without tools.";
const FOLLOW_UP: &str =
    "[kah-reviewer] Delegate context only: nothing new to do. Reply FOLLOWUP_OK without tools.";
const DECISION: &str = "The user decision is to create marker.txt containing exactly RED followed by a newline. Use your native file tool, then make four separate shell tool calls, each writing its number to relayed-1.txt through relayed-4.txt. Do not combine them. Finally report. No subagents or other edits.";
/// kah reviews at commits and claims. After the rebuild, this commit's review
/// runs through the rule set that now carries the decision.
const COMMIT: &str = "[kah-reviewer] Delegate request: commit marker.txt and relayed-1.txt through relayed-4.txt with git in one commit (message: record relayed decision), then reply COMMITTED. No other edits.";

pub(super) fn run(root: &Path) -> Value {
    let socket = std::env::var("HERDR_SOCKET_PATH").unwrap();
    assert!(socket.contains("kah-gate-"));
    let home = PathBuf::from(std::env::var_os("HOME").unwrap());
    let (reviewer, _courier) = start_reviewer(&home, root);
    let codex = selected("codex").then(|| start_codex(&home));
    let claude = selected("claude").then(|| {
        let fixture = crate::claude_support::Fixture::new();
        baseline_commit(fixture.project());
        let pane = start_claude_committer(&home, &fixture, root);
        (pane, fixture)
    });
    let mut panes = Panes(vec![reviewer.clone()]);
    let mut workers = vec![];
    if let Some((pane, fixture, id)) = &codex {
        panes.0.push(pane.clone());
        workers.push(("codex", pane.clone(), id.clone(), fixture.project.clone()));
    }
    if let Some((pane, fixture)) = &claude {
        panes.0.push(pane.clone());
        assert_eq!(ready(pane, "claude"), fixture.id);
        workers.push((
            "claude",
            pane.clone(),
            fixture.id.clone(),
            fixture.project().into(),
        ));
    }
    for (agent, pane, _, _) in &workers {
        pane.prompt(PLAIN).unwrap();
        std::thread::sleep(Duration::from_secs(2));
        ready(pane, agent);
    }
    for (_, _, id, _) in &workers {
        steer(&reviewer, id);
    }
    let results = workers
        .iter()
        .map(|(_, pane, id, project)| prove(pane, id, project))
        .collect::<Vec<_>>();
    if let Some((_, fixture)) = &claude {
        assert!(
            fixture
                .rows()
                .iter()
                .filter(|r| r["type"] == "assistant")
                .all(|r| r["message"]["model"]
                    .as_str()
                    .is_none_or(|m| m.starts_with("claude-sonnet-5-5")))
        );
    }
    let agents = workers.iter().map(|w| w.0).collect::<Vec<_>>();
    json!({"passed":true,"agents":agents,"versions":versions(&home),"fixtureModels":{"codex":"gpt-6.1-sol/low","claude":"claude-sonnet-5-5/low"},"claudeObserver":claude.is_some().then(crate::claude_support::observer::evidence),"privateHerdr":true,"userSteering":results,"reviewerSent":"fixture pane named kah-reviewer in the private Herdr"})
}

/// The tool-batch checkpoint admission needs a HEAD (`COMMIT_HEAD_MISSING
/// stage=admission` otherwise); the empty commit leaves the worker's files alone.
/// Both workers of the 5o-0 journey, Claude and Codex, start from it.
pub(super) fn baseline_commit(project: &Path) {
    let status = Command::new("git")
        .args([
            "-C",
            project.to_str().unwrap(),
            "-c",
            "user.name=fixture",
            "-c",
            "user.email=fixture@example.invalid",
            "-c",
            "core.hooksPath=/dev/null",
            "-c",
            "commit.gpgsign=false",
            "commit",
            "--quiet",
            "--allow-empty",
            "-m",
            "fixture source baseline",
        ])
        .status()
        .unwrap();
    assert!(status.success(), "baseline commit failed");
}

fn start_codex(home: &Path) -> (Pane, crate::fixture::Fixture, String) {
    let (launch, mut fixture) = crate::fixture::prepare(home, "research", false);
    // As for the Claude worker: the commit review needs a HEAD, and the session's
    // first git sample must see it so the worker's commit is a change, not the baseline.
    baseline_commit(&fixture.project);
    let mut command = launch.command(false);
    command
        .args([
            "--no-alt-screen",
            "Reply READY, then wait. Do not use tools or create files.",
        ])
        .env_remove("CODEX_THREAD_ID");
    let pane = Pane::start(&command).unwrap();
    let id = ready(&pane, "codex");
    fixture.sessions.push(id.clone());
    assert!(
        !launch
            .overlay
            .join("app-server-control/app-server-control.sock")
            .exists()
    );
    (pane, fixture, id)
}

/// Keeps a courier's fixture state alive until the journey ends.
enum Courier {
    Codex(#[allow(dead_code)] crate::fixture::Fixture),
    Claude(#[allow(dead_code)] crate::claude_support::Fixture),
}

/// A disposable agent pane renamed `kah-reviewer`: `kah steer` records the relayer
/// from the live Herdr name of the pane it runs in, and research workers cannot
/// reach the Herdr socket, so the fixture issues the host commands in its name.
/// A Claude-only run uses a Claude courier so Codex is never started.
fn start_reviewer(home: &Path, root: &Path) -> (Pane, Courier) {
    const TASK: &str = "You are a disposable courier in a live fixture. Reply READY, then wait. Do not use tools or create files.";
    let (pane, courier) = if selected("codex") {
        let (launch, mut fixture) = crate::fixture::prepare(home, "research", false);
        let mut command = launch.command(false);
        command
            .args(["--no-alt-screen", TASK])
            .env_remove("CODEX_THREAD_ID");
        let pane = Pane::start(&command).unwrap();
        fixture.sessions.push(ready(&pane, "codex"));
        (pane, Courier::Codex(fixture))
    } else {
        let fixture = crate::claude_support::Fixture::new();
        let pane = start_claude_prompt(home, &fixture, root, TASK);
        assert_eq!(ready(&pane, "claude"), fixture.id);
        (pane, Courier::Claude(fixture))
    };
    assert!(
        Command::new("herdr")
            .args(["agent", "rename", &pane.id, "kah-reviewer"])
            .status()
            .unwrap()
            .success()
    );
    (pane, courier)
}

fn steer(reviewer: &Pane, session: &str) {
    let output = Command::new(env!("CARGO_BIN_EXE_kah"))
        .args(["steer", session, DECISION])
        .env("HERDR_PANE_ID", &reviewer.id)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "kah steer {session}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

/// Uncertain supersession routes a Jev-typed steer to the full-context observer
/// (Decision::Verification); both variants carry Jev's judgment.
fn typed_by_jev(id: &str, input: &kah_observer::intake::Input) -> &'static str {
    use kah_observer::intake::Decision;
    match &input.decision {
        Some(Decision::Jev { judgment }) if judgment.backend == "jev" => "jev",
        Some(Decision::Verification { judgment }) if judgment.backend == "jev" => {
            "jev-verification"
        }
        other => panic!(
            "relayed steer for {id} was not typed by Jev: {}",
            format!("{other:?}").chars().take(600).collect::<String>()
        ),
    }
}

/// Seconds since the steer was submitted at which each milestone was first seen.
#[derive(Default, serde::Serialize)]
struct Progress {
    steer_row: Option<u64>,
    decided: Option<(u64, &'static str)>,
    file_4: Option<u64>,
    /// When the steer was resolved, and how: by the verification review (with
    /// its review id), or applied at once on the direct Jev route.
    review_resolved: Option<(u64, String)>,
    /// The accepted rule set version whose input carries the promoted decision.
    rule_set_has_decision: Option<(u64, String)>,
    /// The first review after the promotion that cites that rule set: a native
    /// request, or the worker commit's review (Jev-only commits write no request).
    next_request_has_decision: Option<(u64, String)>,
    #[serde(skip)]
    promoted_at_ms: Option<u64>,
    #[serde(skip)]
    followed_up: bool,
    /// HEAD when the worker was asked to commit.
    #[serde(skip)]
    commit_from: Option<String>,
}
impl Progress {
    fn done(&self) -> bool {
        self.decided.is_some()
            && self.file_4.is_some()
            && self.review_resolved.is_some()
            && self.rule_set_has_decision.is_some()
            && self.next_request_has_decision.is_some()
    }
    fn observe(&mut self, id: &str, project: &Path, t: u64) {
        let base = RunStore::default_state_root().unwrap();
        let Ok(store) = Store::open(&base, id, Duration::ZERO) else {
            return;
        };
        if let Ok(inputs) = store.sources()
            && let Some(input) = inputs.iter().find(|i| i.text == DECISION)
        {
            if self.steer_row.is_none() {
                assert_eq!(input.source.role, SourceRole::User);
                assert_eq!(input.source.origin, Some(SourceOrigin::Relayed));
                assert_eq!(input.source.relayer.as_deref(), Some("kah-reviewer"));
                assert!(inputs.iter().all(|i| i.text != PLAIN));
                self.steer_row = Some(t);
            }
            if self.decided.is_none() && input.decision.is_some() {
                self.decided = Some((t, typed_by_jev(id, input)));
            }
            if self.review_resolved.is_none()
                && let Some(how) = resolution(id, input)
            {
                self.review_resolved = Some((t, how));
                self.promoted_at_ms = Some(kah_observer::session::now_ms());
            }
        }
        if project.join("relayed-4.txt").exists() {
            self.file_4.get_or_insert(t);
        }
        if self.rule_set_has_decision.is_none()
            && self.promoted_at_ms.is_some()
            && let Some(file) = steer_reach::rule_set_with(id, project, DECISION)
        {
            self.rule_set_has_decision = Some((t, file));
        }
        if self.next_request_has_decision.is_none()
            && let Some(since) = self.promoted_at_ms
        {
            let commit = self
                .commit_from
                .as_deref()
                .and_then(|from| head(project).filter(|now| now != from));
            let found = steer_reach::request_citing(id, project, DECISION, since).or_else(|| {
                steer_reach::commit_review_citing(id, project, DECISION, commit.as_deref()?)
            });
            if let Some(file) = found {
                self.next_request_has_decision = Some((t, file));
            }
        }
    }
}

fn head(project: &Path) -> Option<String> {
    let output = Command::new("git")
        .args(["-C", project.to_str()?, "rev-parse", "HEAD"])
        .output()
        .ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).trim().to_owned())
}

/// How the steer was resolved, if it was. A verification-routed steer waits for
/// the review to promote or dismiss it. On the direct Jev route reconciliation
/// applies the steer to the intent history at once (`Decision::records_intent`)
/// and leaves promoted/dismissed false; the rule set check then proves it.
fn resolution(id: &str, input: &kah_observer::intake::Input) -> Option<String> {
    use kah_observer::intake::Decision;
    if input.promoted || input.dismissed {
        let action = if input.promoted {
            "promoted"
        } else {
            "dismissed"
        };
        return Some(format!("{action} by {}", latest_review(id)));
    }
    matches!(&input.decision, Some(decision @ Decision::Jev { .. }) if decision.records_intent())
        .then(|| "applied at once on the direct Jev route".into())
}

/// The review request the observer last completed (`unknown` when none is recorded).
fn latest_review(id: &str) -> String {
    kah_cli::observer_review::state::Store::read(id)
        .ok()
        .flatten()
        .and_then(|s| s.latest)
        .and_then(|v| v["request"].as_str().map(str::to_owned))
        .unwrap_or_else(|| "unknown".into())
}

fn loadavg() -> String {
    fs::read_to_string("/proc/loadavg")
        .unwrap_or_default()
        .split_whitespace()
        .take(3)
        .collect::<Vec<_>>()
        .join(" ")
}

/// What the full-context observer is doing for this worker, for timeout reports.
fn observer_state(id: &str) -> Value {
    let verification = kah_observer::verification::enabled(id).ok();
    let review = kah_cli::observer_review::state::Store::read(id)
        .ok()
        .flatten()
        .map(|state| {
            json!({"pending":state.pending.as_ref().map(|p| json!({"id":p.id,"turn":p.turn,"age_ms":kah_observer::session::now_ms().saturating_sub(p.created_ms)})),
            "latestPurpose":state.latest.as_ref().map(|v| v["purpose"].clone()),"wakes":state.wakes,"observerSession":state.observer.session})
        });
    json!({"verificationEnabled":verification,"review":review,"steerInput":steer_input(id),"reviewResults":review_results(id)})
}

/// The steer's ledger row after review: resolved (promoted/dismissed) or still open.
fn steer_input(id: &str) -> Value {
    let base = RunStore::default_state_root().unwrap();
    let Ok(store) = Store::open(&base, id, Duration::ZERO) else {
        return Value::Null;
    };
    store
        .sources()
        .unwrap_or_default()
        .iter()
        .find(|i| i.text == DECISION)
        .map_or(Value::Null, |i| json!({"promoted":i.promoted,"dismissed":i.dismissed,"decided":i.decision.is_some()}))
}

/// Every review result the observer wrote: purpose, status, gap and intake resolution.
fn review_results(id: &str) -> Value {
    let Ok(directory) = kah_cli::observer_review::state::Store::directory(id) else {
        return Value::Null;
    };
    let mut results = vec![];
    for entry in fs::read_dir(directory.join("results"))
        .into_iter()
        .flatten()
        .flatten()
    {
        let value: Value = fs::read(entry.path())
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or(Value::Null);
        results.push(json!({"file":entry.file_name().to_string_lossy(),"status":value["status"],"gap":value["gap"],"intakeResolution":value["intake_resolution"],"purpose":value["purpose"]}));
    }
    json!(results)
}

/// Why the observer prompt may lack the steer: what the projection holds, its
/// revision against the steer's resolution, and which observer files mention it.
fn prompt_state(id: &str) -> Value {
    let base = RunStore::default_state_root().unwrap();
    let Ok(store) = Store::open(&base, id, Duration::ZERO) else {
        return Value::Null;
    };
    let steer = store
        .sources()
        .unwrap_or_default()
        .into_iter()
        .find(|i| i.text == DECISION);
    let state = store.load().ok().flatten();
    let projection = state
        .as_ref()
        .and_then(|s| kah_observer::engine::projection(s).ok().flatten());
    let items = projection.as_ref().map(|p| {
        p.preserve
            .iter()
            .chain(&p.frontier)
            .chain(&p.validation)
            .map(|i| i.text.chars().take(50).collect::<String>())
            .collect::<Vec<_>>()
    });
    json!({
        "steerSource": steer.as_ref().map(|i| json!({"record":i.source.record,"promoted":i.promoted,"dismissed":i.dismissed,"ruleSetResolution":i.rule_set_resolution})),
        "stateLoaded": state.is_some(),
        "projection": projection.as_ref().map(|p| json!({"revision":p.revision,"sourceHead":p.source_head.record,"pendingSequence":p.source_head.pending_sequence,"compacted":p.compaction.is_some(),"promptBytes":p.rendered_prompt().len(),"items":items})),
        "requestCitations": steer_reach::request_citations(id),
    })
}

fn prove(pane: &Pane, id: &str, project: &Path) -> Value {
    let started = Instant::now();
    // Haiku verification, the rule set rebuild and the commit review in turn.
    let deadline = started + Duration::from_secs(900);
    let mut progress = Progress::default();
    loop {
        progress.observe(id, project, started.elapsed().as_secs());
        // A later native event gives the observer its next review request,
        // which now carries the promoted decision.
        if progress.promoted_at_ms.is_some() && !progress.followed_up {
            progress.followed_up = true;
            pane.prompt(FOLLOW_UP).unwrap();
        }
        if progress.rule_set_has_decision.is_some() && progress.commit_from.is_none() {
            progress.commit_from = Some(head(project).unwrap_or_default());
            pane.prompt(COMMIT).unwrap();
        }
        if progress.done() {
            assert_eq!(
                fs::read_to_string(project.join("marker.txt")).unwrap(),
                "RED\n"
            );
            let kind = progress.decided.map(|d| d.1);
            return json!({"session":id,"origin":"relayed","relayer":"kah-reviewer","verbatim":true,"typedByJev":true,"decision":kind,"plainPrefixNotUserIntent":true,"milestonesSecondsAfterSteer":progress,"loadavgAtEnd":loadavg(),"passed":true});
        }
        assert!(
            Instant::now() < deadline,
            "reviewer steering/ledger/review did not complete for {id} after {} s: {}",
            started.elapsed().as_secs(),
            json!({"milestonesSecondsAfterSteer":progress,"missing":missing(&progress),"observer":observer_state(id),"promptState":prompt_state(id),"loadavg":loadavg()})
        );
        std::thread::sleep(Duration::from_millis(200));
    }
}

fn missing(progress: &Progress) -> Vec<&'static str> {
    [
        (progress.steer_row.is_none(), "steer not in the ledger"),
        (progress.decided.is_none(), "no decision on the steer"),
        (
            progress.file_4.is_none(),
            "worker never wrote relayed-4.txt",
        ),
        (
            progress.review_resolved.is_none(),
            "verification review never resolved the steer",
        ),
        (
            progress.rule_set_has_decision.is_none(),
            "current rule set lacks the decision",
        ),
        (
            progress.next_request_has_decision.is_none(),
            "no review after the promotion (request or worker commit) cites the rule set that carries the decision",
        ),
    ]
    .into_iter()
    .filter_map(|(absent, what)| absent.then_some(what))
    .collect()
}
