use kah_core::{
    intent_history::{SourceProvider, SourceRole},
    unattended::{Limits, RunControl},
};
use kah_run::intake::user_bundle;
use std::fs;

#[test]
fn only_operator_task_and_steers_enter_the_unattended_user_bundle() {
    let root = std::env::temp_dir().join(format!(
        "kah-run-intake-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir(&root).unwrap();
    let mut control = RunControl::new("Build a sum function", Limits::default()).unwrap();
    control.steer("Also accept negative values").unwrap();
    let bundle = user_bundle(&root, "run-test", &control).unwrap();
    assert_eq!(bundle.source().provider(), SourceProvider::Bundle);
    assert_eq!(bundle.authoritative_records().len(), 2);
    assert!(
        bundle
            .records()
            .iter()
            .all(|r| r.role() == SourceRole::User)
    );
    assert_eq!(bundle.records()[0].content(), control.task());
    assert_eq!(bundle.records()[1].id(), "kah-user:run-test:1");
    fs::remove_dir_all(root).unwrap();
}
