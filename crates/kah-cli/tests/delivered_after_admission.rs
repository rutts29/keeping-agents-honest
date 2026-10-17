//! A worker can commit a change whose review defers
//! the obligation, but no delivered-change review ran. The commit's admission
//! waited behind the cold current_rules review, the worker's Stop came first,
//! and its turn-end check found no deferral yet. Admission now asks the
//! worker's own transcript whether the commit's turn has ended. These replay
//! the orders deterministically on a transcript-shaped file.
use kah_cli::observer_review::{delivered, native_turn_end, state::Store};
use serde_json::{Value, json};
use std::path::PathBuf;

const HEAD: &str = "c2f4760000000000000000000000000000000000";
const SESSION: &str = "11111111-2222-4333-8444-555555555555";
/// git %ct of the commit, 2026-10-17T19:00:00Z, in ms (second precision).
const COMMITTED_MS: u64 = 1_792_263_600_000;

struct Fixture {
    root: PathBuf,
    transcript: PathBuf,
}
impl Fixture {
    fn new(name: &str) -> Self {
        let worker = format!("delivered-{name}-{}", std::process::id());
        let root = Store::directory(&worker).unwrap();
        kah_core::run_store::ensure_private_directory(&root.join("sources")).unwrap();
        let transcript = root.join("worker.jsonl");
        std::fs::write(&transcript, "").unwrap();
        Self { root, transcript }
    }
    fn row(&self, row: Value) {
        use std::io::Write;
        let mut file = std::fs::OpenOptions::new()
            .append(true)
            .open(&self.transcript)
            .unwrap();
        writeln!(file, "{row}").unwrap();
    }
    fn prompt(&self, text: &str, at: &str) {
        self.row(json!({"type":"user","sessionId":SESSION,"isSidechain":false,"timestamp":at,"message":{"role":"user","content":text}}));
    }
    fn reply(&self, at: &str) {
        self.row(json!({"type":"assistant","sessionId":SESSION,"isSidechain":false,"timestamp":at,"message":{"role":"assistant","content":[{"type":"text","text":"Done."}]}}));
    }
    /// Claude Code 2.1.302 order: Stop hook summary, then turn_duration.
    fn stop(&self, at: &str) {
        self.row(json!({"type":"system","subtype":"stop_hook_summary","sessionId":SESSION,"isSidechain":false,"timestamp":at,"hookCount":1}));
        self.row(json!({"type":"system","subtype":"turn_duration","sessionId":SESSION,"isSidechain":false,"timestamp":at,"durationMs":4000}));
    }
    fn marker(&self) -> Option<Value> {
        std::fs::read(self.root.join(delivered::MARKER))
            .ok()
            .map(|b| serde_json::from_slice(&b).unwrap())
    }
    /// The worker's turn-end check (claims job -> delivered::schedule -> queue_due).
    fn turn_end_check(&self) -> bool {
        let marker = self.marker();
        delivered::due(marker.as_ref(), HEAD, None, None)
            && delivered::queue_once(&self.root, "t1", HEAD, marker.as_ref().unwrap()).unwrap()
    }
    /// Commit admission: record the deferral, then run the check again when the
    /// transcript shows the commit's turn ended (delivered::admitted).
    fn admit(&self) -> bool {
        kah_cli::unattended_report::save(
            &self.root,
            delivered::MARKER,
            &json!({"head":HEAD,"rules":["r-beta"]}),
        )
        .unwrap();
        let text = std::fs::read_to_string(&self.transcript).unwrap();
        delivered::turn_ended_since(&native_turn_end::claude(&text, SESSION), Some(COMMITTED_MS))
            && self.turn_end_check()
    }
    fn queued(&self) -> usize {
        std::fs::read_dir(self.root.join("queued"))
            .map(|d| {
                d.flatten()
                    .filter(|e| e.path().extension().is_some_and(|x| x == "json"))
                    .count()
            })
            .unwrap_or(0)
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(self.root.parent().unwrap());
    }
}
/// Turn 1: the worker writes OMEGA and commits at 19:00:00.4 (git %ct 19:00:00).
fn commit_turn(f: &Fixture) {
    f.prompt(
        "Write the answer and commit it.",
        "2026-10-17T18:59:56.000Z",
    );
    f.reply("2026-10-17T19:00:00.400Z");
}

#[test]
fn stop_before_admission_queues_exactly_one_delivered_change_review() {
    let f = Fixture::new("stop-first");
    commit_turn(&f);
    f.stop("2026-10-17T19:00:02.000Z");
    assert!(!f.turn_end_check(), "no deferral is recorded yet");
    assert!(f.admit(), "admission must run the missed turn-end check");
    assert_eq!(f.queued(), 1);
}

#[test]
fn admission_before_stop_is_unchanged_and_queues_one_review() {
    let f = Fixture::new("admission-first");
    commit_turn(&f);
    assert!(!f.admit(), "the commit's turn has not ended");
    assert_eq!(f.queued(), 0);
    f.stop("2026-10-17T19:00:02.000Z");
    assert!(f.turn_end_check());
    assert_eq!(f.queued(), 1);
}

#[test]
fn a_later_turn_does_not_hide_the_commit_turns_end() {
    // Codex review: turn 2 starts before turn 1's delayed Stop handler and
    // before turn 1's admission. Turn 1's end stays in the transcript.
    let f = Fixture::new("later-turn");
    commit_turn(&f);
    f.stop("2026-10-17T19:00:02.000Z");
    f.prompt("Next task.", "2026-10-17T19:00:03.000Z");
    assert!(
        !f.turn_end_check(),
        "turn 1's delayed handler: no deferral yet"
    );
    assert!(f.admit());
    assert_eq!(f.queued(), 1);
}

#[test]
fn replayed_checks_never_queue_a_second_review() {
    let f = Fixture::new("replay");
    commit_turn(&f);
    f.stop("2026-10-17T19:00:02.000Z");
    assert!(f.admit());
    assert!(!f.admit());
    assert!(!f.turn_end_check());
    f.prompt("More.", "2026-10-17T19:00:05.000Z");
    f.stop("2026-10-17T19:00:09.000Z");
    assert!(!f.turn_end_check());
    assert_eq!(f.queued(), 1);
}

#[test]
fn a_previous_turn_end_in_the_commits_own_second_counts_by_design() {
    // Git's commit time has one-second precision. A previous turn that ended
    // at 19:00:00.200, before a commit at 19:00:00.400 in a new turn, cannot be
    // told apart: chosen behaviour is to count it (an early review, never a
    // missed one). A turn end in the previous second never counts.
    let f = Fixture::new("same-second");
    f.prompt("Earlier task.", "2026-10-17T18:59:50.000Z");
    f.stop("2026-10-17T18:59:59.800Z");
    let text = std::fs::read_to_string(&f.transcript).unwrap();
    assert!(!delivered::turn_ended_since(
        &native_turn_end::claude(&text, SESSION),
        Some(COMMITTED_MS)
    ));
    f.stop("2026-10-17T19:00:00.200Z");
    commit_turn(&f);
    assert!(f.admit(), "same-second earlier turn end counts by design");
    assert_eq!(f.queued(), 1);
}

#[test]
fn only_the_workers_main_thread_turn_ends_count() {
    let f = Fixture::new("filter");
    commit_turn(&f);
    f.row(json!({"type":"system","subtype":"turn_duration","sessionId":SESSION,"isSidechain":true,"timestamp":"2026-10-17T19:00:02.000Z"}));
    f.row(json!({"type":"system","subtype":"turn_duration","sessionId":"other","isSidechain":false,"timestamp":"2026-10-17T19:00:02.000Z"}));
    let text = std::fs::read_to_string(&f.transcript).unwrap();
    assert!(native_turn_end::claude(&text, SESSION).is_empty());
    f.stop("2026-10-17T19:00:02.000Z");
    let text = std::fs::read_to_string(&f.transcript).unwrap();
    assert_eq!(
        native_turn_end::claude(&text, SESSION),
        [COMMITTED_MS + 2_000]
    );
}

#[test]
fn an_unreadable_or_turnless_transcript_keeps_todays_behaviour() {
    // No new failure path: without a readable transcript or a parsable
    // turn_duration row, admission rechecks nothing (the old behaviour).
    let state = |provider: &str, project: &str| -> kah_cli::observer_review::state::State {
        serde_json::from_value(json!({"worker":{"provider":provider,"session":SESSION,"project":project,"pane":null,"socket":null},"observer":{"session":null,"pane":null,"name":"fixture","workspace":"/tmp","model":"claude-haiku-5-5","effort":"high","transcript":null},"pending":null,"latest":null,"last_verified_turn":null,"checkpoint":null,"deferrals":[],"wakes":0,"native_tokens":0,"stopped":false})).unwrap()
    };
    let home = std::env::temp_dir().join(format!("delivered-home-{}", std::process::id()));
    let missing = home.join("no-such-project");
    let ends = native_turn_end::worker(&home, &state("claude", missing.to_str().unwrap()));
    assert!(
        ends.is_empty(),
        "an unresolvable transcript yields no turn ends"
    );
    assert!(native_turn_end::worker(&home, &state("codex", "/tmp")).is_empty());
    let garbage = "not json\n{\"type\":\"system\",\"subtype\":\"turn_duration\",\"sessionId\":\"S\",\"timestamp\":\"yesterday\"}\n";
    assert!(native_turn_end::claude(garbage, "S").is_empty());
    assert!(!delivered::turn_ended_since(&[], Some(COMMITTED_MS)));
    assert!(!delivered::turn_ended_since(&[COMMITTED_MS + 5_000], None));
    let f = Fixture::new("turnless");
    commit_turn(&f);
    assert!(!f.admit(), "no turn_duration row: nothing is rechecked");
    assert_eq!(f.queued(), 0);
}

#[test]
fn only_interactive_sessions_catch_up_an_unattended_run_keeps_todays_path() {
    // A kah run worker has a run-completion checkpoint ahead that reviews every
    // rule on the delivered head, so its late order queues nothing new here.
    let home = std::env::temp_dir().join(format!("delivered-run-{}", std::process::id()));
    let id = format!("run-delivered-{}", std::process::id());
    let worktree = home.join(&id).join("worktree");
    let base = kah_core::run_store::RunStore::default_state_root()
        .unwrap()
        .join("runs");
    let record = kah_run::store::Record {
        project: home.join("source"),
        domain: "research".into(),
        worktree: worktree.clone(),
        model: None,
        effort: None,
        native: Some(kah_run::native::Native::new("claude").unwrap()),
        control: kah_core::unattended::RunControl::new("fixture", Default::default()).unwrap(),
    };
    drop(kah_run::store::Store::create(&base, &id, record).unwrap());
    assert!(
        !delivered::catch_up_applies(&worktree),
        "unattended kah run"
    );
    assert!(
        delivered::catch_up_applies(&home.join("repos/research/project")),
        "interactive"
    );
    // The same late order as 5f: Stop, then admission.
    let interactive = Fixture::new("interactive-late");
    commit_turn(&interactive);
    interactive.stop("2026-10-17T19:00:02.000Z");
    assert!(interactive.admit());
    assert_eq!(
        interactive.queued(),
        1,
        "interactive: exactly one delivered-change review"
    );
    let unattended = Fixture::new("unattended-late");
    commit_turn(&unattended);
    unattended.stop("2026-10-17T19:00:02.000Z");
    let late = delivered::catch_up_applies(&worktree) && unattended.admit();
    assert!(!late);
    assert_eq!(
        unattended.queued(),
        0,
        "unattended: nothing new via this path"
    );
    std::fs::remove_dir_all(kah_run::store::Store::path(&base, &id).unwrap()).unwrap();
    let _ = std::fs::remove_dir_all(home);
}
