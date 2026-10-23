use kah_cli::observer_review::{
    events, queue,
    state::{State, Store},
};
use serde_json::json;
#[test]
fn busy_checkpoint_transfer_is_queued_once_and_cannot_repeat_itself_after_review() {
    let worker = format!("checkpoint-handoff-unit-{}", std::process::id());
    let store = Store::open(&worker).unwrap();
    let state:State=serde_json::from_value(json!({"worker":{"provider":"codex","session":worker,"project":"/tmp","pane":null,"socket":null},"observer":{"session":null,"pane":null,"name":"fixture","workspace":"/tmp","model":"gpt-6.1-sol","effort":"low","transcript":null},"pending":null,"latest":null,"last_verified_turn":null,"checkpoint":null,"deferrals":[],"wakes":0,"native_tokens":0,"stopped":false})).unwrap();
    store.save(&state).unwrap();
    let root = store.directory.join("checkpoint-events");
    kah_core::run_store::ensure_private_directory(&root).unwrap();
    let whole = json!({"kind":"user_gate","at_ms":1,"meaning":"whole original event"});
    kah_cli::unattended_report::save(&root, "first.json", &whole).unwrap();
    // run() waits in its detached admission phase.
    // This held-lock test checks the independent durable transfer seam; both
    // admission continuations are covered by review_queue's threaded regressions.
    events::transfer(&worker).unwrap();
    assert!(
        !events::pending(&worker).unwrap(),
        "the event was already durable in the review queue"
    );
    events::transfer(&worker).unwrap();
    let jobs = queue::entries(&worker).unwrap();
    assert_eq!(jobs.len(), 1);
    assert_eq!(jobs[0].1.trigger["events"][0], whole);
    let directory = store.directory.parent().unwrap().to_path_buf();
    drop(store);
    std::fs::remove_dir_all(directory).unwrap();
}

#[test]
fn an_event_written_while_the_running_job_holds_its_lock_is_not_stranded() {
    // The tag job wrote the worker commit's checkpoint event
    // while __checkpoint-worker was draining (delivering current_rules), so
    // hook::spawn found the job lock held and started nothing. The running job
    // then exited without a second transfer and the commit was never reviewed.
    let worker = format!("checkpoint-wakeup-unit-{}", std::process::id());
    let store = Store::open(&worker).unwrap();
    let state:State=serde_json::from_value(json!({"worker":{"provider":"codex","session":worker,"project":"/tmp","pane":null,"socket":null},"observer":{"session":null,"pane":null,"name":"fixture","workspace":"/tmp","model":"gpt-6.1-sol","effort":"low","transcript":null},"pending":null,"latest":null,"last_verified_turn":null,"checkpoint":null,"deferrals":[],"wakes":0,"native_tokens":0,"stopped":false})).unwrap();
    store.save(&state).unwrap();
    let root = store.directory.join("checkpoint-events");
    kah_core::run_store::ensure_private_directory(&root).unwrap();
    kah_cli::unattended_report::save(&root, "first.json", &json!({"kind":"first","at_ms":1}))
        .unwrap();
    let lock = store.directory.join("job-__checkpoint-worker.lock");
    let open = || {
        std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&lock)
            .unwrap()
    };
    let lease = open();
    lease.try_lock().unwrap();
    let mut drains = 0;
    events::drain_all(&worker, &lease, |_| {
        drains += 1;
        if drains == 1 {
            let event = json!({"kind":"user_gate","at_ms":2,"meaning":"written during the drain"});
            kah_cli::unattended_report::save(&root, "second.json", &event).unwrap();
            // hook::spawn would skip here: the running job holds the lock.
            assert!(open().try_lock().is_err());
        }
        Ok(())
    })
    .unwrap();
    assert!(
        !events::pending(&worker).unwrap(),
        "the commit event was stranded"
    );
    assert_eq!(queue::entries(&worker).unwrap().len(), 2);
    assert_eq!(drains, 2);
    let directory = store.directory.parent().unwrap().to_path_buf();
    drop(store);
    std::fs::remove_dir_all(directory).unwrap();
}
