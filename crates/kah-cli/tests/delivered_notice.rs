//! A delivered-change review can find drift
//! (checkpoint_violated), but its notice was only queued as a hook reminder.
//! The review runs after the worker's turn ended, so an idle interactive worker
//! fired no hook and never saw it. That notice is now pushed into the pane;
//! kah run workers and every other checkpoint keep the queued reminder.
use kah_cli::observer_review::delivered;
use serde_json::json;

#[test]
fn only_a_delivered_change_checkpoint_of_an_interactive_worker_is_pushed() {
    let home = std::env::temp_dir().join(format!("delivered-notice-{}", std::process::id()));
    let interactive = home.join("repos/research/project");
    let wake = delivered::wake(
        "t1",
        "c2f4760",
        &json!({"head":"c2f4760","rules":["r-beta"]}),
    );
    assert!(
        delivered::push_notice(&wake.trigger, &interactive),
        "an idle interactive worker fires no hook to take a queued reminder"
    );
    // Any other checkpoint trigger keeps the queued reminder.
    let completion = json!({"kind":"run_completion","completion_key":{}});
    assert!(!delivered::push_notice(&completion, &interactive));
    let claim = json!({"review_class":"gate","events":[{"kind":"native_phase_claim"}]});
    assert!(!delivered::push_notice(&claim, &interactive));
    // An unattended kah run worker keeps today's path.
    let id = format!("run-notice-{}", std::process::id());
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
    assert!(!delivered::push_notice(&wake.trigger, &worktree));
    std::fs::remove_dir_all(kah_run::store::Store::path(&base, &id).unwrap()).unwrap();
    let _ = std::fs::remove_dir_all(home);
}
