//! The Stop-before-admission race on an interactive Codex
//! worker. Codex 0.162.1 runs the Stop hooks inside the turn and only then
//! writes the turn's task_complete row to the rollout, as Claude writes
//! turn_duration after its Stop hooks. Admission reads those rows. These replay
//! the orders deterministically on a rollout-shaped file.
use kah_cli::observer_review::{delivered, native_turn_end, state::Store};
use serde_json::{Value, json};
use std::path::PathBuf;

const HEAD: &str = "c2f4760000000000000000000000000000000000";
const SESSION: &str = "00000000-0000-4000-8000-000000000001";
/// git %ct of the commit, 2026-10-17T19:00:00Z, in ms (second precision).
const COMMITTED_MS: u64 = 1_792_263_600_000;

struct Fixture {
    root: PathBuf,
    codex_home: PathBuf,
    rollout: PathBuf,
}
impl Fixture {
    fn new(name: &str) -> Self {
        let worker = format!("delivered-codex-{name}-{}", std::process::id());
        let root = Store::directory(&worker).unwrap();
        kah_core::run_store::ensure_private_directory(&root.join("sources")).unwrap();
        let codex_home = root.join("codex-home");
        let day = codex_home.join("sessions/2026/10/17");
        std::fs::create_dir_all(&day).unwrap();
        let rollout = day.join(format!("rollout-2026-10-17T18-59-50-{SESSION}.jsonl"));
        std::fs::write(&rollout, "").unwrap();
        let f = Self {
            root,
            codex_home,
            rollout,
        };
        f.event(
            "session_meta",
            "2026-10-17T18:59:50.000Z",
            json!({"id":SESSION}),
        );
        f
    }
    fn event(&self, kind: &str, at: &str, payload: Value) {
        use std::io::Write;
        let mut file = std::fs::OpenOptions::new()
            .append(true)
            .open(&self.rollout)
            .unwrap();
        let row = if kind == "session_meta" {
            json!({"timestamp":at,"type":kind,"payload":payload})
        } else {
            json!({"timestamp":at,"type":"event_msg","payload":{"type":kind,"turn_id":payload}})
        };
        writeln!(file, "{row}").unwrap();
    }
    fn prompt(&self, turn: &str, at: &str) {
        self.event("task_started", at, json!(turn));
    }
    /// Codex 0.162.1 order: Stop hooks inside the turn, then task_complete.
    fn stop(&self, turn: &str, at: &str) {
        self.event("task_complete", at, json!(turn));
    }
    fn ends(&self) -> Vec<u64> {
        native_turn_end::codex_rollout(&self.codex_home, SESSION)
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
    /// rollout shows the commit's turn ended (delivered::admitted).
    fn admit(&self) -> bool {
        kah_cli::unattended_report::save(
            &self.root,
            delivered::MARKER,
            &json!({"head":HEAD,"rules":["r-beta"]}),
        )
        .unwrap();
        delivered::turn_ended_since(&self.ends(), Some(COMMITTED_MS)) && self.turn_end_check()
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

#[test]
fn codex_stop_before_admission_queues_exactly_one_delivered_change_review() {
    let f = Fixture::new("stop-first");
    f.prompt("t1", "2026-10-17T18:59:56.000Z");
    f.stop("t1", "2026-10-17T19:00:02.000Z");
    assert!(!f.turn_end_check(), "no deferral is recorded yet");
    assert!(f.admit(), "admission must run the missed turn-end check");
    assert_eq!(f.queued(), 1);
}

#[test]
fn codex_admission_before_stop_is_unchanged_and_queues_one_review() {
    let f = Fixture::new("admission-first");
    f.prompt("t1", "2026-10-17T18:59:56.000Z");
    assert!(!f.admit(), "the commit's turn has not ended");
    assert_eq!(f.queued(), 0);
    f.stop("t1", "2026-10-17T19:00:02.000Z");
    assert!(f.turn_end_check());
    assert_eq!(f.queued(), 1);
}

#[test]
fn codex_a_later_turn_does_not_hide_the_commit_turns_end() {
    let f = Fixture::new("later-turn");
    f.prompt("t1", "2026-10-17T18:59:56.000Z");
    f.stop("t1", "2026-10-17T19:00:02.000Z");
    f.prompt("t2", "2026-10-17T19:00:03.000Z");
    assert!(
        !f.turn_end_check(),
        "turn 1's delayed handler: no deferral yet"
    );
    assert!(f.admit());
    assert_eq!(f.queued(), 1);
}

#[test]
fn codex_replayed_checks_never_queue_a_second_review() {
    let f = Fixture::new("replay");
    f.prompt("t1", "2026-10-17T18:59:56.000Z");
    f.stop("t1", "2026-10-17T19:00:02.000Z");
    assert!(f.admit());
    assert!(!f.admit());
    f.prompt("t2", "2026-10-17T19:00:05.000Z");
    f.stop("t2", "2026-10-17T19:00:09.000Z");
    assert!(!f.turn_end_check());
    assert_eq!(f.queued(), 1);
}

#[test]
fn codex_only_completed_turns_of_this_rollout_count() {
    let f = Fixture::new("filter");
    f.prompt("t1", "2026-10-17T18:59:56.000Z");
    f.event("turn_aborted", "2026-10-17T19:00:01.000Z", json!("t1"));
    f.event("token_count", "2026-10-17T19:00:01.500Z", json!("t1"));
    assert!(
        f.ends().is_empty(),
        "an aborted turn is not a completed turn"
    );
    let other = f
        .rollout
        .with_file_name("rollout-2026-10-17T18-59-50-other.jsonl");
    std::fs::write(
        &other,
        json!({"timestamp":"2026-10-17T19:00:02.000Z","type":"event_msg","payload":{"type":"task_complete"}})
            .to_string()
            + "\n",
    )
    .unwrap();
    assert!(f.ends().is_empty(), "another thread's rollout never counts");
    f.stop("t1", "2026-10-17T19:00:02.000Z");
    assert_eq!(f.ends(), [COMMITTED_MS + 2_000]);
    assert!(native_turn_end::codex("not json\n").is_empty());
}

#[test]
fn codex_a_missing_rollout_keeps_todays_behaviour() {
    let f = Fixture::new("missing");
    std::fs::remove_file(&f.rollout).unwrap();
    assert!(f.ends().is_empty());
    assert!(native_turn_end::codex_rollout(&f.root.join("no-such-home"), SESSION).is_empty());
    assert!(!f.admit(), "no rollout: nothing is rechecked");
    assert_eq!(f.queued(), 0);
}

#[test]
fn codex_worker_turn_ends_come_from_the_bound_native_home() {
    use kah_observer::session;
    let f = Fixture::new("binding");
    let id = format!("00000000-0000-4000-8001-{:012}", std::process::id());
    let rollout = f
        .rollout
        .with_file_name(format!("rollout-2026-10-17T18-59-50-{id}.jsonl"));
    std::fs::write(
        &rollout,
        json!({"timestamp":"2026-10-17T19:00:02.000Z","type":"event_msg","payload":{"type":"task_complete"}})
            .to_string()
            + "\n",
    )
    .unwrap();
    let base = kah_core::run_store::RunStore::default_state_root().unwrap();
    let binding = session::Binding {
        session: id.clone(),
        project: f.root.clone(),
        codex_home: f.codex_home.clone(),
        native: "codex".into(),
        model: "gpt-6.1-sol".into(),
        effort: "low".into(),
        timeout_secs: 90,
        judge: Default::default(),
        jev_model: "jev-latest".into(),
    };
    session::Store::open_wait(&base, &id)
        .unwrap()
        .save(&session::State::new(binding))
        .unwrap();
    let state: kah_cli::observer_review::state::State = serde_json::from_value(json!({"worker":{"provider":"codex","session":id,"project":f.root,"pane":null,"socket":null},"observer":{"session":null,"pane":null,"name":"fixture","workspace":"/tmp","model":"gpt-6.1-sol","effort":"low","transcript":null},"pending":null,"latest":null,"last_verified_turn":null,"checkpoint":null,"deferrals":[],"wakes":0,"native_tokens":0,"stopped":false})).unwrap();
    let ends = native_turn_end::worker(&f.root.join("unused-home"), &state);
    let _ = std::fs::remove_dir_all(session::Store::directory(&base, &id));
    assert_eq!(ends, [COMMITTED_MS + 2_000]);
}
