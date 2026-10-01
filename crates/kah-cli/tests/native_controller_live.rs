#[allow(dead_code)]
#[path = "terminal_support/herdr_probe.rs"]
mod herdr_probe;
mod native_controller_support;
#[allow(dead_code)]
#[path = "terminal_support/processes.rs"]
mod processes;
#[allow(dead_code)]
#[path = "terminal_support/pty.rs"]
mod pty;
use serde_json::json;
use std::{
    fs,
    path::PathBuf,
    process::Command,
    time::{Duration, Instant},
};

#[test]
#[ignore = "gVisor checkpoint/import/verification path diagnostic; no model call"]
fn verification_uses_imported_commit() {
    native_controller_support::verification_uses_imported_commit();
}

#[test]
#[ignore = "public kah run: full Codex and Claude sessions, verified completion in Herdr"]
fn native_runs_complete_in_their_own_panes() {
    for agent in ["claude", "codex"] {
        if std::env::var("KAH_GATE_AGENT").is_ok_and(|selected| selected != agent) {
            continue;
        }
        journey(agent);
    }
}
fn journey(agent: &str) {
    let probe = herdr_probe::setup();
    let command = format!(
        "HERDR_CONFIG_PATH={} herdr --session {}",
        pty::quote(probe.root.join("config.toml").to_str().unwrap()),
        pty::quote(&probe.name)
    );
    let mut terminal = pty::Terminal::start(&command, &probe.root.join("terminal.log"));
    terminal.wait_for("KAH_HERDR_SHELL_READY");
    terminal.submit(&format!("KAH_NATIVE_RUN_PROBE={} KAH_NATIVE_RUN_AGENT={} {} --exact native_run_probe_entrypoint --nocapture > {} 2>&1", pty::quote(probe.root.to_str().unwrap()), pty::quote(agent), pty::quote(std::env::current_exe().unwrap().to_str().unwrap()), pty::quote(probe.root.join("helper.log").to_str().unwrap())));
    let deadline = Instant::now() + Duration::from_secs(300);
    let evidence = loop {
        terminal.respond_to_cursor_queries();
        if let Ok(bytes) = fs::read(probe.root.join("result.json")) {
            break serde_json::from_slice::<serde_json::Value>(&bytes).unwrap();
        }
        if Instant::now() >= deadline {
            break json!({"passed":false,"failure":"native run helper timed out"});
        }
        std::thread::sleep(Duration::from_millis(200));
    };
    let _ = Command::new("herdr")
        .args(["session", "stop", &probe.name, "--json"])
        .output();
    drop(terminal);
    native_controller_support::cleanup(&probe.root);
    if evidence["passed"] != true {
        let diagnostic = std::env::temp_dir().join(format!(
            "kah-native-run-failure-{agent}-{}.log",
            std::process::id()
        ));
        fs::copy(probe.root.join("helper.log"), &diagnostic).unwrap();
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&diagnostic, fs::Permissions::from_mode(0o600)).unwrap();
        panic!("native run journey failed: {}", diagnostic.display());
    }
    fs::write(
        PathBuf::from(std::env::var_os("KAH_GATE_EVIDENCE").unwrap())
            .join(format!("native-run-{agent}.json")),
        serde_json::to_vec_pretty(&evidence).unwrap(),
    )
    .unwrap();
}

#[test]
fn native_run_probe_entrypoint() {
    let Some(root) = std::env::var_os("KAH_NATIVE_RUN_PROBE") else {
        return;
    };
    let root = PathBuf::from(root);
    let agent = std::env::var("KAH_NATIVE_RUN_AGENT").unwrap();
    let result = std::panic::catch_unwind(|| native_controller_support::run(&root, &agent));
    fs::write(
        root.join("result.json"),
        result
            .unwrap_or_else(|_| json!({"passed":false}))
            .to_string(),
    )
    .unwrap();
}
