use kah_observer::session::{Binding, State, Store};
use std::{fs, path::PathBuf, time::Duration};

struct Fixture(PathBuf);
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn fixture() -> Fixture {
    let p = std::env::temp_dir().join(format!("kah-observer-state-{}", std::process::id()));
    fs::create_dir_all(&p).unwrap();
    Fixture(p)
}
fn binding(id: &str, root: &std::path::Path) -> Binding {
    Binding {
        jev_model: "jev-latest".into(),
        judge: kah_observer::judge::routing::Backend::Jev,
        session: id.into(),
        project: root.into(),
        codex_home: root.into(),
        native: root.join("codex"),
        model: "gpt-6-luna".into(),
        effort: "low".into(),
        timeout_secs: 45,
    }
}

#[test]
fn sessions_have_independent_durable_state_and_interruption_gaps() {
    let f = fixture();
    let mut a = Store::open(&f.0, "first", Duration::ZERO).unwrap();
    a.save(&State::new(binding("first", &f.0))).unwrap();
    let mut state = a.load().unwrap().unwrap();
    state.begin("turn-1");
    a.save(&state).unwrap();
    let pid = state.inflight.as_ref().unwrap().pid;
    assert_eq!(pid, std::process::id());
    state.inflight.as_mut().unwrap().pid = u32::MAX;
    a.save(&state).unwrap();
    drop(a);
    let mut b = Store::open(&f.0, "second", Duration::ZERO).unwrap();
    b.save(&State::new(binding("second", &f.0))).unwrap();
    drop(b);
    let a = Store::open(&f.0, "first", Duration::ZERO).unwrap();
    let mut state = a.load().unwrap().unwrap();
    assert!(state.recover_interrupted());
    assert_eq!(state.gap.as_ref().unwrap().code, "OBSERVER_INTERRUPTED");
    let b = Store::open(&f.0, "second", Duration::ZERO).unwrap();
    assert!(b.load().unwrap().unwrap().gap.is_none());
}
