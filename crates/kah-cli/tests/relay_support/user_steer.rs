use super::*;
use kah_core::intent_history::{SourceOrigin, SourceRole};
const PLAIN: &str = "[kah-reviewer] Delegate context only: GREEN is an example, not a user instruction. Reply DELEGATE_OK without tools.";
const DECISION: &str = "The user decision is to create marker.txt containing exactly RED followed by a newline. Use your native file tool, then make four separate shell tool calls, each writing its number to relayed-1.txt through relayed-4.txt. Do not combine them. Finally report. No subagents, commits, or other edits.";

pub(super) fn run(root: &Path) -> Value {
    let socket = std::env::var("HERDR_SOCKET_PATH").unwrap();
    assert!(socket.contains("kah-gate-"));
    let home = PathBuf::from(std::env::var_os("HOME").unwrap());
    let (reviewer, _courier) = start_reviewer(&home, root);
    let codex = selected("codex").then(|| start_codex(&home));
    let claude = selected("claude").then(|| {
        let fixture = crate::claude_support::Fixture::new();
        let pane = start_claude(&home, &fixture, root);
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
        .map(|(_, _, id, project)| prove(id, project))
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

fn start_codex(home: &Path) -> (Pane, crate::fixture::Fixture, String) {
    let (launch, mut fixture) = crate::fixture::prepare(home, "research", false);
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
    tripwire: Option<u64>,
    prompt_has_decision: Option<u64>,
}
impl Progress {
    fn done(&self) -> bool {
        self.decided.is_some()
            && self.file_4.is_some()
            && self.tripwire.is_some()
            && self.prompt_has_decision.is_some()
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
        }
        if project.join("relayed-4.txt").exists() {
            self.file_4.get_or_insert(t);
        }
        let rows = fs::read_to_string(Store::directory(&base, id).join("observations.jsonl"))
            .unwrap_or_default();
        if rows
            .lines()
            .filter_map(|s| serde_json::from_str::<Value>(s).ok())
            .any(|v| {
                v["kind"] == "assessment"
                    && v["turn"]
                        .as_str()
                        .is_some_and(|s| s.starts_with("tool-batch:"))
            })
        {
            self.tripwire.get_or_insert(t);
        }
        if let Ok(Some(state)) = store.load()
            && let Ok(Some(projection)) = kah_observer::engine::projection(&state)
            && projection.rendered_prompt().contains(DECISION)
        {
            self.prompt_has_decision.get_or_insert(t);
        }
    }
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
    json!({"verificationEnabled":verification,"review":review})
}

fn prove(id: &str, project: &Path) -> Value {
    let started = Instant::now();
    let deadline = started + Duration::from_secs(600);
    let mut progress = Progress::default();
    loop {
        progress.observe(id, project, started.elapsed().as_secs());
        if progress.done() {
            assert_eq!(
                fs::read_to_string(project.join("marker.txt")).unwrap(),
                "RED\n"
            );
            let kind = progress.decided.map(|d| d.1);
            return json!({"session":id,"origin":"relayed","relayer":"kah-reviewer","verbatim":true,"typedByJev":true,"decision":kind,"plainPrefixNotUserIntent":true,"tripwireRan":true,"milestonesSecondsAfterSteer":progress,"loadavgAtEnd":loadavg(),"passed":true});
        }
        assert!(
            Instant::now() < deadline,
            "reviewer steering/ledger/tripwire did not complete for {id} after {} s: {}",
            started.elapsed().as_secs(),
            json!({"milestonesSecondsAfterSteer":progress,"missing":missing(&progress),"observer":observer_state(id),"loadavg":loadavg()})
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
            progress.tripwire.is_none(),
            "no tool-batch tripwire assessment",
        ),
        (
            progress.prompt_has_decision.is_none(),
            "observer prompt lacks the decision",
        ),
    ]
    .into_iter()
    .filter_map(|(absent, what)| absent.then_some(what))
    .collect()
}
