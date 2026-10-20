//! #51: a resumed observer appends to the transcript of its old process. The
//! old rows of the same request are not a receipt for the new process.
use kah_cli::observer_review::{
    delivery, delivery_text,
    state::{Pending, Purpose, State, Store},
};
use serde_json::{Value, json};
use std::{
    fs,
    io::Write,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    process::Command,
    time::{Duration, Instant},
};

const WORKER: &str = "cold-resume-worker";
const SESSION: &str = "5b0c2a4e-7d61-4f0e-9a51-3c8f2d6e1b07";
const REQUEST: &str = "review-1000000-1";
// Herdr shows the new pane idle while the resumed process starts.
const SHIM: &str = "#!/bin/sh
case \"$1 $2\" in
'pane get') printf '{\"result\":{\"pane\":{\"pane_id\":\"%s\",\"agent\":\"claude\",\"agent_status\":\"idle\"}}}' \"$3\";;
'pane read') echo 'native TUI';;
*) exit 2;;
esac
";

#[test]
fn a_resumed_observer_is_not_bound_by_its_old_receipt() {
    let root = std::env::temp_dir().join(format!("kah-cold-resume-{}", std::process::id()));
    fs::create_dir_all(&root).unwrap();
    fs::write(root.join("herdr"), SHIM).unwrap();
    fs::set_permissions(root.join("herdr"), fs::Permissions::from_mode(0o700)).unwrap();
    let output = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "cold_resume_child", "--nocapture"])
        .env("KAH_COLD_RESUME_FIXTURE", &root)
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
fn cold_resume_child() {
    let Some(root) = std::env::var_os("KAH_COLD_RESUME_FIXTURE") else {
        return;
    };
    let root = PathBuf::from(root);
    let prompt = format!("kah review request {REQUEST} review");
    // The old process received this request; its turn then ended.
    let native = root.join("native.jsonl");
    append(&native, &prompt);
    let history = fs::metadata(&native).unwrap().len();
    let state = setup(&root, &native);
    let before = delivery::History::before(&state);
    // binding::ensure cleared the PID. The new process starts slowly, then
    // its SessionStart names the same file by another path and it receives
    // the request from its launch arguments.
    let start = Duration::from_secs(2);
    let (resumed, link) = (native.clone(), root.join("sessions.jsonl"));
    std::thread::spawn(move || {
        std::thread::sleep(start);
        std::os::unix::fs::symlink(&resumed, &link).unwrap();
        let store = Store::open_wait(WORKER).unwrap();
        let mut state = store.load().unwrap().unwrap();
        state.observer.pid = Some(std::process::id());
        state.observer.transcript = Some(link);
        store.save(&state).unwrap();
        append(&resumed, &format!("kah review request {REQUEST} review"));
    });
    let started = Instant::now();
    delivery::cold(&state, &prompt, before).unwrap();
    assert!(
        started.elapsed() >= start,
        "receipt taken from the old process's rows after {:?}",
        started.elapsed()
    );
    let current: Value = serde_json::from_slice(
        &fs::read(
            Store::directory(WORKER)
                .unwrap()
                .join("delivery-outcome/current.json"),
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(current["progress"]["cursor"], history);
}

fn append(path: &Path, prompt: &str) {
    let mut file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .unwrap();
    for row in [
        json!({"type":"user","sessionId":SESSION,"message":{"content":delivery_text::cold(prompt)}}),
        json!({"type":"assistant","sessionId":SESSION,"message":{"stop_reason":"end_turn","content":[{"type":"text","text":"Working."}]}}),
    ] {
        writeln!(file, "{row}").unwrap();
    }
}

fn setup(root: &Path, native: &Path) -> State {
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
    state.observer.session = Some(SESSION.into());
    state.observer.transcript = Some(native.to_path_buf());
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
