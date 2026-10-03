use kah_cli::{
    observer_review::{
        checkpoint,
        state::{Pending, Purpose, Request, State, Store},
    },
    relay_store::Endpoint,
};
use serde_json::json;
#[test]
fn checkpoint_receipts_cover_only_the_reviewed_commit_and_intent_and_preserve_failed_nudges() {
    let worker = format!("checkpoint-unit-{}", std::process::id());
    let home = std::env::temp_dir().join(&worker);
    saved_binding(&worker, &home);
    let mut state = State::new(
        Endpoint {
            provider: "codex".into(),
            session: worker.clone(),
            project: home.clone(),
            pane: None,
            socket: None,
        },
        &home,
    )
    .unwrap();
    assert_eq!(state.observer.model, "gpt-6-astra");
    assert_eq!(state.observer.effort, "high");
    let store = Store::open(&worker).unwrap();
    let request = Request {
        id: "review-unit".into(),
        worker: worker.clone(),
        purpose: Purpose::Checkpoint,
        turn: "t".into(),
        head: Some("abc".into()),
        intent: json!({"revision":2}),
        trigger: json!({}),
        documents: vec![],
        created_ms: 0,
    };
    let result = json!({"assessment":{"passed":false,"unproven":["integration"],"deferrals":[{"id":"d","evidence":"requires phase 3"}]}});
    checkpoint::record(&store, &mut state, &request, &result).unwrap();
    assert_eq!(checkpoint::covers(&state, "abc", 2), Some(false));
    assert_eq!(checkpoint::covers(&state, "def", 2), None);
    assert_eq!(checkpoint::covers(&state, "abc", 3), None);
    assert_eq!(state.deferrals.len(), 1);
    let reminder: serde_json::Value =
        serde_json::from_slice(&std::fs::read(store.directory.join("reminder.json")).unwrap())
            .unwrap();
    assert_eq!(reminder["purpose"], "checkpoint");
    drop(store);
    previous_finding(&worker);
    let store = Store::open(&worker).unwrap();
    state.checkpoint.as_mut().unwrap()["passed"] = true.into();
    assert_eq!(checkpoint::covers(&state, "abc", 2), Some(true));
    state.pending = Some(Pending::from(&request));
    assert_eq!(checkpoint::covers(&state, "abc", 2), None);
    let directory = store.directory.clone();
    drop(store);
    std::fs::remove_dir_all(directory.parent().unwrap()).unwrap();
}

fn previous_finding(worker: &str) {
    let store = Store::open(worker).unwrap();
    kah_cli::unattended_report::save(
        &store.directory,
        "reminder.json",
        &json!({"purpose":"drift","turn":"previous","text":"Confirmed integration failure"}),
    )
    .unwrap();
    drop(store);
    let delivered = kah_cli::observer_review::consume::take(worker, "PostToolUse", "new")
        .unwrap()
        .unwrap();
    assert!(
        delivered["hookSpecificOutput"]["additionalContext"]
            .as_str()
            .unwrap()
            .contains("previous turn")
    );
    assert!(
        delivered["hookSpecificOutput"]["additionalContext"]
            .as_str()
            .unwrap()
            .contains("Confirmed integration failure")
    );
}

fn saved_binding(worker: &str, home: &std::path::Path) {
    let base = kah_core::run_store::RunStore::default_state_root().unwrap();
    let mut primary =
        kah_observer::session::Store::open(&base, worker, std::time::Duration::ZERO).unwrap();
    primary
        .save(&kah_observer::session::State::new(
            kah_observer::session::Binding {
                session: worker.into(),
                project: home.into(),
                codex_home: home.join("codex"),
                native: home.join("native"),
                model: "gpt-6-astra".into(),
                effort: "high".into(),
                timeout_secs: 45,
                jev_model: "jev-latest".into(),
                judge: kah_observer::judge::routing::Backend::Jev,
            },
        ))
        .unwrap();
    drop(primary);
}
