use kah_core::unattended::{Limits, RunControl};
use kah_run::{
    commands,
    store::{MAX_STATE_BYTES, Record, Store},
};
use std::{fs, path::PathBuf};

fn root(case: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!("kah-run-limits-{case}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    root
}

fn record(model: &str) -> Record {
    Record {
        native: None,
        project: PathBuf::from("/fixture/project"),
        domain: "mine".into(),
        worktree: PathBuf::from("/fixture/run"),
        model: Some(model.into()),
        effort: None,
        control: RunControl::new("Implement sum", Limits::default()).unwrap(),
    }
}

fn serialized_len(record: &Record) -> usize {
    serde_json::to_vec(record).unwrap().len() + 1
}

#[test]
fn a_state_exactly_at_the_limit_saves_and_loads_and_one_byte_more_is_refused() {
    let root = root("boundary");
    let padding = MAX_STATE_BYTES - serialized_len(&record(""));
    let mut store = Store::create(&root, "run-one", record("")).unwrap();
    store.save(&record(&"a".repeat(padding))).unwrap();
    assert_eq!(
        fs::metadata(root.join("run-one/state.json")).unwrap().len() as usize,
        MAX_STATE_BYTES
    );
    store.load().expect("a saved state always loads");
    let before = fs::read(root.join("run-one/state.json")).unwrap();

    let error = store.save(&record(&"a".repeat(padding + 1))).unwrap_err();
    assert!(error.contains("RUN_STATE_SIZE"), "{error}");
    assert_eq!(fs::read(root.join("run-one/state.json")).unwrap(), before);
    store.load().expect("the last good state is intact");
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn steering_that_would_exceed_the_limit_fails_and_leaves_the_run_readable() {
    let root = root("steer");
    drop(Store::create(&root, "run-one", record("m")).unwrap());
    drop(Store::create(&root, "run-two", record("m")).unwrap());
    let message = "x".repeat(100 * 1024);
    let mut refused = None;
    for attempt in 0..40 {
        if let Err(error) = commands::steer(&root, "run-one", &message) {
            refused = Some((attempt, error));
            break;
        }
    }
    let (attempt, error) = refused.expect("some steering message must cross the limit");
    assert!(error.contains("RUN_STATE_SIZE"), "{error}");
    assert!(
        attempt >= 15,
        "the limit is about 2 MiB, not {attempt} messages"
    );

    let run = commands::view(&root, "run-one").expect("the run stays readable");
    assert_eq!(run.control.inputs().len(), attempt + 1);
    let listed: Vec<String> = commands::list(&root)
        .expect("listing stays possible")
        .into_iter()
        .map(|(id, _)| id)
        .collect();
    assert_eq!(listed, ["run-one", "run-two"]);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn an_oversized_first_record_leaves_no_run_directory() {
    let root = root("create");
    let padding = MAX_STATE_BYTES - serialized_len(&record(""));
    let error = Store::create(&root, "run-big", record(&"a".repeat(padding + 1)))
        .err()
        .expect("oversized first state is refused");
    assert!(error.contains("RUN_STATE_SIZE"), "{error}");
    assert!(!root.join("run-big").exists());
    Store::create(&root, "run-big", record("m")).expect("the id is still usable");
    fs::remove_dir_all(root).unwrap();
}
