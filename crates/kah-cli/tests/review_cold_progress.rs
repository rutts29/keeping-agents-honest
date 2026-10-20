//! F-COLD-BIND-15S: a cold observer that is visibly working is not stalled.
use kah_cli::observer_review::{
    delivery,
    state::{Pending, Purpose, State, Store},
};
use std::{
    fs,
    os::unix::fs::PermissionsExt,
    path::Path,
    process::Command,
    time::{Duration, Instant},
};

const WORKER: &str = "cold-progress-worker";
const REQUEST: &str = "review-1000000000017-1";
// Herdr reports the native agent's turn state; the binding is still absent.
const SHIM: &str = "#!/bin/sh
case \"$1 $2\" in
'pane get') if [ -e \"$HOME/working\" ]; then s=working; else s=idle; fi
  printf '{\"result\":{\"pane\":{\"pane_id\":\"%s\",\"agent\":\"claude\",\"agent_status\":\"%s\"}}}' \"$3\" \"$s\";;
'pane read') echo 'native TUI';;
*) exit 2;;
esac
";

#[test]
fn a_working_cold_observer_keeps_the_binding_window_open() {
    let root = std::env::temp_dir().join(format!("kah-cold-progress-{}", std::process::id()));
    fs::create_dir_all(&root).unwrap();
    fs::write(root.join("herdr"), SHIM).unwrap();
    fs::set_permissions(root.join("herdr"), fs::Permissions::from_mode(0o700)).unwrap();
    let output = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "cold_progress_child", "--nocapture"])
        .env("KAH_COLD_FIXTURE", &root)
        .env("HOME", &root)
        .env("XDG_STATE_HOME", root.join("state"))
        .env("PATH", format!("{}:/usr/bin:/bin", root.display()))
        .output()
        .unwrap();
    fs::remove_dir_all(&root).unwrap();
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn cold_progress_child() {
    let Some(root) = std::env::var_os("KAH_COLD_FIXTURE") else {
        return;
    };
    let root = std::path::PathBuf::from(root);
    let state = setup(&root);
    fs::write(root.join("working"), "").unwrap();
    let working = root.join("working");
    // The native turn runs longer than the old fixed 15 s window, then ends.
    let busy = Duration::from_secs(18);
    std::thread::spawn(move || {
        std::thread::sleep(busy);
        fs::remove_file(working).unwrap();
    });
    let started = Instant::now();
    let error = delivery::cold(
        &state,
        &format!("kah review request {REQUEST} review"),
        None,
    )
    .unwrap_err()
    .to_string();
    assert!(error.starts_with("REVIEW_NATIVE_UNBOUND"), "{error}");
    assert!(
        started.elapsed() >= busy,
        "unbound reported after {:?} while the native observer was still working",
        started.elapsed()
    );
}

fn setup(root: &Path) -> State {
    use kah_observer::session;
    let base = kah_core::run_store::RunStore::default_state_root().unwrap();
    let binding = session::Binding {
        session: WORKER.into(),
        project: root.to_path_buf(),
        codex_home: root.join("codex"),
        native: "/nonexistent/native".into(),
        model: "claude-sonnet-5-5".into(),
        effort: "low".into(),
        timeout_secs: 90,
        judge: Default::default(),
        jev_model: "jev-latest".into(),
    };
    let mut primary = session::Store::open(&base, WORKER, Duration::ZERO).unwrap();
    primary.save(&session::State::new(binding)).unwrap();
    drop(primary);
    let endpoint = kah_cli::relay_store::Endpoint {
        provider: "claude".into(),
        session: WORKER.into(),
        project: root.to_path_buf(),
        pane: Some("w0:worker".into()),
        socket: Some(root.join("herdr.sock")),
    };
    let mut state = State::new(endpoint, root).unwrap();
    state.observer.pane = Some("w0:observer".into());
    state.pending = Some(Pending {
        id: REQUEST.into(),
        purpose: Purpose::Checkpoint,
        turn: "turn".into(),
        head: None,
        created_ms: 0,
        native_tokens_before: None,
    });
    Store::open(WORKER).unwrap().save(&state).unwrap();
    state
}
