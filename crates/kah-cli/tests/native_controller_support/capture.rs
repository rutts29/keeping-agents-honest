use super::*;
use std::{io::Write, os::unix::fs::OpenOptionsExt};

pub(super) fn capture(home: &Path, directory: &Path, record: &Record, report: &Value) {
    if record.control.status() == RunStatus::Completed || report["tests"].is_null() {
        return;
    }
    let native = record.native.as_ref().unwrap();
    let Some(session) = native.session.as_ref() else {
        return;
    };
    let candidate = kah_run::worktree::Worktree::load(&directory.join("candidate.json")).unwrap();
    // This fixture deliberately handles only its two public marker files.
    if candidate.changed_paths().unwrap() != vec!["containment.txt", "marker.txt"] {
        return;
    }
    assert_eq!(
        fs::read_to_string(candidate.path.join("marker.txt"))
            .unwrap()
            .trim(),
        "NATIVE_RUN_OK"
    );
    assert_eq!(
        fs::read_to_string(candidate.path.join("containment.txt"))
            .unwrap()
            .trim(),
        "LOGIN_HIDDEN"
    );
    let tests = serde_json::from_value(report["tests"].clone()).unwrap();
    let state =
        kah_observer::session::Store::read(&RunStore::default_state_root().unwrap(), session)
            .unwrap()
            .unwrap();
    assert_eq!(state.binding.project, record.worktree);
    let projection = kah_observer::engine::projection(&state).unwrap().unwrap();
    let mut history = kah_cli::native_history::History::open(
        home,
        &directory.join("fixture-capture"),
        &native.provider,
        &record.worktree,
        session,
    )
    .unwrap();
    let actions = history.completion_activity().unwrap();
    let evidence = kah_cli::unattended_observer::candidate_evidence(
        &actions,
        &candidate,
        &tests,
        native.online,
    )
    .unwrap();
    let path = std::env::temp_dir().join(format!("kah-completion-replay-{session}.json"));
    let value = json!({"projection":projection,"evidence":evidence,"jevModel":state.binding.jev_model,"provider":native.provider});
    let mut file = fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .mode(0o600)
        .open(&path)
        .unwrap();
    file.write_all(serde_json::to_string_pretty(&value).unwrap().as_bytes())
        .unwrap();
    eprintln!("private completion replay: {}", path.display());
}
