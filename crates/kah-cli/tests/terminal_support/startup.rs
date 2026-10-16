use super::pty::Terminal;
use crate::fixture::Fixture;
use kah_codex::control::Control;
use kah_core::run_store::RunStore;
use kah_observer::{engine, session::Store};
use serde_json::{Value, json};
use std::{fs, path::Path, time::Duration};

pub fn verify(home: &Path, f: &Fixture, id: &str, terminal: &Terminal) -> Value {
    let text = terminal.output();
    for warning in [
        "Running without the shared background server",
        "loading hooks from both",
    ] {
        assert!(!text.contains(warning), "{warning}");
    }
    let state = Store::read(&RunStore::default_state_root().unwrap(), id)
        .unwrap()
        .unwrap();
    assert!(state.gap.is_none(), "{:?}", state.gap);
    let projection = engine::projection(&state).unwrap().unwrap();
    assert!(
        projection
            .frontier
            .iter()
            .any(|i| i.text.contains("HERDR_WORKS"))
    );
    assert!(
        fs::symlink_metadata(f.overlay.join("hooks.json"))
            .unwrap()
            .is_file()
    );
    let hooks: Value =
        serde_json::from_slice(&fs::read(f.overlay.join("hooks.json")).unwrap()).unwrap();
    let config = fs::read_to_string(f.overlay.join("config.toml")).unwrap();
    assert!(!config.contains("[[hooks."));
    let global = home.join(".codex/app-server-control/app-server-control.sock");
    let mut client = Control::connect_daemon(&global, Duration::from_secs(20)).unwrap();
    let loaded = client.request("thread/loaded/list", json!({})).unwrap();
    let is_loaded = loaded["data"].as_array().unwrap().iter().any(|v| v == id);
    assert!(
        !is_loaded,
        "embedded fixture unexpectedly loaded on global daemon"
    );
    let metadata = client
        .request("thread/read", json!({"threadId":id,"includeTurns":false}))
        .unwrap();
    let overlay_socket = f.overlay.join("app-server-control/app-server-control.sock");
    let local = Control::connect_daemon(&overlay_socket, Duration::from_secs(2));
    let socket = local.is_ok();
    assert!(
        !socket,
        "embedded TUI unexpectedly has an overlay control socket"
    );
    let delivery = send_probe(id, terminal);
    json!({"kahSend":"delegate message delivered through Herdr; embedded TUI has no control socket","kahSendDelivery":delivery,"warningsAbsent":true,"observerAttached":true,"hookFileRegular":true,"mergedEventNames":hooks["hooks"].as_object().unwrap().keys().collect::<Vec<_>>(),"configContainsOnlyHookState":true,"mode":"embedded","globalDaemonLoaded":is_loaded,"globalHistoryReadable":metadata["thread"]["id"]==id,"overlayControlSocketConnects":socket,"globalSocketAlias":global,"globalSocketTarget":fs::read_link(&global).ok(),"overlaySocket":overlay_socket})
}

fn send_probe(id: &str, terminal: &Terminal) -> Value {
    let sender = std::process::Command::new(env!("CARGO_BIN_EXE_kah"))
        .args([
            "send",
            id,
            "Visibility probe",
            "--sender",
            "gate-probe",
            "--receipt",
            "3f-visibility",
        ])
        .output()
        .unwrap();
    // Since 985814f (10-06) a TUI without a control socket receives delegate
    // messages through native Herdr agent prompt, never as user authority.
    assert!(
        sender.status.success(),
        "{}",
        String::from_utf8_lossy(&sender.stderr)
    );
    let delivery: Value = serde_json::from_slice(&sender.stdout).unwrap();
    assert_eq!(delivery["transport"], "herdr");
    assert_eq!(delivery["status"], "submitted");
    wait_shown(terminal, "gate-probe] Visibility probe");
    delivery
}

fn wait_shown(terminal: &Terminal, text: &str) {
    let deadline = std::time::Instant::now() + Duration::from_secs(45);
    while !terminal.output().contains(text) {
        assert!(
            std::time::Instant::now() < deadline,
            "embedded TUI did not show {text}"
        );
        std::thread::sleep(Duration::from_millis(200));
    }
}
