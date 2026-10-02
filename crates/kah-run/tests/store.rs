use kah_core::unattended::{Limits, RunControl};
use kah_run::store::{Record, Store};
use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};
static NEXT: AtomicU64 = AtomicU64::new(0);
struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        Self(std::env::temp_dir().join(format!(
            "kah-run-store-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        )))
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn record() -> Record {
    Record {
        native: None,
        project: PathBuf::from("/fixture/project"),
        domain: "mine".into(),
        worktree: PathBuf::from("/fixture/run"),
        model: Some("gpt-6.1-sol".into()),
        effort: Some("high".into()),
        control: RunControl::new("Implement sum", Limits::default()).unwrap(),
    }
}
#[test]
fn steering_survives_reopen_and_two_writers_cannot_overwrite_each_other() {
    let f = Fixture::new();
    let mut store = Store::create(&f.0, "run-one", record()).unwrap();
    assert!(Store::open(&f.0, "run-one").is_err());
    let mut state = store.load().unwrap();
    state.control.steer("Keep the API").unwrap();
    store.save(&state).unwrap();
    drop(store);
    let restored = Store::open(&f.0, "run-one").unwrap().load().unwrap();
    assert_eq!(restored.control.inputs()[1].text, "Keep the API");
    assert_eq!(restored.model.as_deref(), Some("gpt-6.1-sol"));
    assert_eq!(restored.effort.as_deref(), Some("high"));
}
#[test]
fn traversal_existing_ids_and_state_symlinks_are_rejected() {
    use std::os::unix::fs::symlink;
    let f = Fixture::new();
    assert!(Store::create(&f.0, "../escape", record()).is_err());
    drop(Store::create(&f.0, "run-one", record()).unwrap());
    assert!(Store::create(&f.0, "run-one", record()).is_err());
    fs::remove_file(f.0.join("run-one/state.json")).unwrap();
    fs::write(f.0.join("canary"), "untouched").unwrap();
    symlink(f.0.join("canary"), f.0.join("run-one/state.json")).unwrap();
    assert!(
        Store::open(&f.0, "run-one")
            .unwrap()
            .save(&record())
            .is_err()
    );
    assert_eq!(fs::read_to_string(f.0.join("canary")).unwrap(), "untouched");
}
