use kah_cli::{
    completion_checkpoint::{self, Status},
    completion_review::Key,
    observer_review::{
        checkpoint, missing_report, queue, source_store,
        state::{Pending, Purpose, Request, State, Store},
    },
    unattended_report::save,
};
use kah_run::verification::Receipt;
use serde_json::{Value, json};
use std::{fs, os::unix::fs::PermissionsExt, path::PathBuf, process::Command};

struct Fixture {
    worker: String,
    root: PathBuf,
    request: Request,
    key: Key,
    receipt: Receipt,
}
impl Fixture {
    fn new() -> Self {
        let worker = format!("retirement-{}", kah_cli::relay_store::nonce());
        let store = Store::open(&worker).unwrap();
        let root = store.directory.clone();
        kah_core::run_store::ensure_private_directory(root.parent().unwrap()).unwrap();
        let key = Key {
            instance: "controller".into(),
            commit: "sha".into(),
            user_sequence: 1,
            action_digest: "actions".into(),
            command: "integration".into(),
            online: false,
        };
        let request = Request {
            id: "review-1".into(),
            worker: worker.clone(),
            purpose: Purpose::Checkpoint,
            turn: "turn".into(),
            head: Some(key.commit.clone()),
            intent: json!({"revision":2}),
            trigger: json!({"events":[{"kind":"run_completion","completion_key":key,"intent_revision":2}]}),
            documents: vec![],
            created_ms: 0,
        };
        let state: State = serde_json::from_value(json!({
            "worker":{"provider":"codex","session":worker,"project":root,"pane":null,"socket":null},
            "observer":{"session":null,"pane":null,"name":"observer","workspace":root.join("native"),"model":"gpt-6.1-sol","effort":"low","transcript":null},
            "pending":Pending::from(&request),"latest":null,"last_verified_turn":null,
            "checkpoint":null,"deferrals":[],"wakes":0,"native_tokens":0,"stopped":false
        })).unwrap();
        store.save(&state).unwrap();
        source_store::save(&root.join("requests"), "review-1.json", &json!(request)).unwrap();
        save(&root, "config.json", &json!({"enabled":true,"provider":"codex","worker":worker,"turn":"turn","pid":null,"process_stamp":null})).unwrap();
        save(
            &root,
            "completion-checkpoint-request.json",
            &json!({"key":key,"revision":2,"invalidation":null}),
        )
        .unwrap();
        let receipt = Receipt {
            commit: key.commit.clone(),
            command: key.command.clone(),
            success: true,
            exit_code: Some(0),
            elapsed_ms: 1,
            output: "verified".into(),
            output_truncated: false,
        };
        Self {
            worker,
            root,
            request,
            key,
            receipt,
        }
    }
    fn poll(&self) -> Status {
        completion_checkpoint::request(&self.root, &self.worker, &self.key, 2, &self.receipt)
            .unwrap()
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        fs::remove_dir_all(self.root.parent().unwrap()).unwrap();
    }
}

#[test]
fn retired_completion_is_terminal_and_reviewed_completion_still_passes() {
    let f = Fixture::new();
    missing_report::retire(&f.worker, &f.request.id, &missing_report::NO_REPORT).unwrap();
    let result: Value = source_store::read(&f.root.join("results/review-1.json")).unwrap();
    assert_eq!(result["status"], "not_reviewed");
    assert_eq!(result["gap"]["code"], "REVIEW_OBSERVER_NO_REPORT");
    assert_eq!(result["gap"]["stage"], "observer");
    for _ in 0..2 {
        let Status::Failed(failed) = f.poll() else {
            panic!("retired completion must be Failed");
        };
        assert_eq!(failed, result);
    }
    assert!(queue::entries(&f.worker).unwrap().is_empty());
    assert!(!f.root.join("checkpoint-events").exists());
    other_deliveries_remain_pending(&f);
}

fn other_deliveries_remain_pending(f: &Fixture) {
    for revision in [2, 3] {
        let mut key = f.key.clone();
        if revision == 2 {
            key.user_sequence += 1;
        }
        save(
            &f.root,
            "completion-checkpoint-request.json",
            &json!({"key":key,"revision":revision,"invalidation":null}),
        )
        .unwrap();
        assert!(matches!(
            completion_checkpoint::request(&f.root, &f.worker, &key, revision, &f.receipt).unwrap(),
            Status::Pending
        ));
    }
}

#[test]
fn a_normal_reviewed_completion_still_passes() {
    let normal = Fixture::new();
    let store = Store::open(&normal.worker).unwrap();
    let mut state = store.load().unwrap().unwrap();
    checkpoint::record(
        &store,
        &mut state,
        &normal.request,
        &json!({"assessment":{"passed":true}}),
    )
    .unwrap();
    state.pending = None;
    store.save(&state).unwrap();
    drop(store);
    assert!(matches!(normal.poll(), Status::Passed));
}

#[test]
fn undelivered_retirement_keeps_the_trigger_eligible_for_fresh_admission() {
    if std::env::var_os("KAH_RETIREMENT_CHILD").is_none() {
        let home =
            std::env::temp_dir().join(format!("retirement-home-{}", kah_cli::relay_store::nonce()));
        fs::create_dir_all(home.join(".local/bin")).unwrap();
        let program = home.join(".local/bin/kah");
        fs::write(&program, "#!/bin/sh\nexit 0\n").unwrap();
        fs::set_permissions(&program, fs::Permissions::from_mode(0o700)).unwrap();
        let status = Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "undelivered_retirement_keeps_the_trigger_eligible_for_fresh_admission",
            ])
            .env("KAH_RETIREMENT_CHILD", "1")
            .env("HOME", &home)
            .status()
            .unwrap();
        fs::remove_dir_all(home).unwrap();
        assert!(status.success());
        return;
    }
    let f = Fixture::new();
    queue::enqueue(
        &f.worker,
        Purpose::Checkpoint,
        "turn",
        f.request.trigger.clone(),
    )
    .unwrap();
    missing_report::retire(&f.worker, &f.request.id, &missing_report::UNDELIVERED).unwrap();
    let state = Store::read(&f.worker).unwrap().unwrap();
    assert!(state.pending.is_none());
    assert!(state.checkpoint.is_none());
    assert!(matches!(f.poll(), Status::Pending));
    let (wake, paths) = queue::claim_batch(&f.worker).unwrap().unwrap();
    assert_eq!(wake.purpose, Purpose::Checkpoint);
    assert_eq!(wake.trigger["events"], f.request.trigger["events"]);
    assert_eq!(paths.len(), 1);
    let stored: Value = source_store::read(&paths[0]).unwrap();
    assert_eq!(stored["trigger"], f.request.trigger);
}

#[test]
fn a_retired_completion_remains_terminal_after_other_observer_jobs() {
    let mut f = Fixture::new();
    missing_report::retire(&f.worker, &f.request.id, &missing_report::NO_REPORT).unwrap();
    let result: Value = source_store::read(&f.root.join("results/review-1.json")).unwrap();
    let later = Request {
        id: "review-later".into(),
        ..f.request.clone()
    };
    for pending in [Some(Pending::from(&later)), None] {
        let store = Store::open(&f.worker).unwrap();
        let mut state = store.load().unwrap().unwrap();
        state.pending = pending;
        state.checkpoint = Some(json!({"head":"another-commit","intent_revision":2,"passed":true}));
        state.latest = state.checkpoint.clone();
        store.save(&state).unwrap();
        drop(store);
        let Status::Failed(failed) = f.poll() else {
            panic!("another observer job must not hide the retired completion");
        };
        assert_eq!(failed, result);
    }
    f.key.instance = "restarted-controller".into();
    f.key.action_digest = "later-evidence".into();
    assert!(matches!(f.poll(), Status::Failed(_)));
}

#[test]
fn a_retirement_is_not_a_review_verdict_for_a_later_delivery() {
    use kah_cli::observer_review::option1::carry;
    let f = Fixture::new();
    missing_report::retire(&f.worker, &f.request.id, &missing_report::NO_REPORT).unwrap();
    let store = Store::open(&f.worker).unwrap();
    let mut state = store.load().unwrap().unwrap();
    let mut key = f.key.clone();
    key.user_sequence += 1;
    let trigger =
        json!({"events":[{"kind":"run_completion","completion_key":key,"intent_revision":2}]});
    let carried = carry::admit(&store, &mut state, &key.commit, 2, "next-turn", &trigger).unwrap();
    assert!(
        carried.is_none(),
        "a missing report cannot satisfy fresh review admission"
    );
    let saved = Store::read(&f.worker).unwrap().unwrap();
    assert!(saved.checkpoint.is_none());
    assert_eq!(saved.latest.unwrap()["status"], "not_reviewed");
    drop(store);
    assert!(matches!(f.poll(), Status::Failed(_)));
}
