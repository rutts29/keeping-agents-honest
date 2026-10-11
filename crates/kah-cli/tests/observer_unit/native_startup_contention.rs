use super::*;
use kah_core::{
    intent_history::{GoalRevision, History, SourceProvider, SourceRef, SourceRole},
    projection::{ProjectionConfig, project},
    run_store::RunStore,
};
use kah_observer::session::Store;
use std::{fs, path::Path, time::Duration};
fn keeper(root: &Path) -> Result<Store> {
    Ok(Store::open(
        root,
        "01a11893-4c7e-73a0-8da6-18b8b16e276c",
        Duration::ZERO,
    )?)
}
fn writer(root: &Path) -> (History, RunStore) {
    let source = SourceRef::new(
        SourceProvider::Bundle,
        "worker",
        "user0",
        SourceRole::User,
        "sha256:fixture",
    );
    let history = History::new(GoalRevision::new(
        1,
        "verify the delivered pipeline",
        vec![source],
    ))
    .unwrap();
    let projection = project(&history, ProjectionConfig::default()).unwrap();
    let writer = RunStore::create(
        root,
        root.join("project"),
        "startup-case",
        &history,
        &projection,
    )
    .unwrap();
    (history, writer)
}
#[test]
fn retained_startup_busy_codes_defer_the_poll_and_recover_after_both_writers_release() {
    let root = std::env::temp_dir().join(format!("kah-startup-contention-{}", std::process::id()));
    fs::create_dir_all(root.join("project")).unwrap();
    let busy = keeper(&root).unwrap();
    assert!(
        poll(|| keeper(&root)).unwrap().is_none(),
        "The retained OBSERVER_BUSY startup report must remain pending, not stop the run"
    );
    drop(busy);
    assert!(poll(|| keeper(&root)).unwrap().is_some());
    let (history, busy) = writer(&root);
    assert!(
        poll(|| RunStore::open(&root, root.join("project"), "startup-case").map_err(Into::into))
            .unwrap()
            .is_none(),
        "The retained STORE_BUSY startup report must remain pending"
    );
    drop(busy);
    let recovered =
        poll(|| RunStore::open(&root, root.join("project"), "startup-case").map_err(Into::into))
            .unwrap()
            .unwrap()
            .recover()
            .unwrap();
    assert_eq!(recovered.history, history);
    assert!(poll::<()>(|| Err("RUN_OBSERVER_SCOPE stage=run_sync".into())).is_err());
    fs::remove_dir_all(root).unwrap();
}
