#[allow(dead_code)]
#[path = "terminal_support/herdr_probe.rs"]
mod herdr_probe;
#[allow(dead_code)]
#[path = "terminal_support/processes.rs"]
mod processes;
#[allow(dead_code)]
#[path = "terminal_support/pty.rs"]
mod pty;
#[path = "native_pane_support/mod.rs"]
mod support;
use serde_json::json;
use std::{
    fs,
    process::Command,
    time::{Duration, Instant},
};

#[test]
#[ignore = "full native Codex session in its own Herdr pane on a Mine worktree"]
fn native_codex_worktree_pane() {
    let probe = herdr_probe::setup();
    let version = Command::new("codex").arg("--version").output().unwrap();
    let command = format!(
        "HERDR_CONFIG_PATH={} herdr --session {}",
        pty::quote(probe.root.join("config.toml").to_str().unwrap()),
        pty::quote(&probe.name)
    );
    let mut terminal = pty::Terminal::start(&command, &probe.root.join("terminal.log"));
    terminal.wait_for("KAH_HERDR_SHELL_READY");
    terminal.submit(&format!(
        "KAH_NATIVE_PANE_PROBE={} {} --exact native_pane_probe_entrypoint --nocapture > {} 2>&1",
        pty::quote(probe.root.to_str().unwrap()),
        pty::quote(std::env::current_exe().unwrap().to_str().unwrap()),
        pty::quote(probe.root.join("helper.log").to_str().unwrap())
    ));
    let deadline = Instant::now() + Duration::from_secs(180);
    let evidence = loop {
        terminal.respond_to_cursor_queries();
        if let Ok(bytes) = fs::read(probe.root.join("result.json")) {
            break serde_json::from_slice::<serde_json::Value>(&bytes).unwrap();
        }
        if Instant::now() >= deadline {
            break json!({"passed":false,"failure":"native pane helper timed out"});
        }
        std::thread::sleep(Duration::from_millis(200));
    };
    let _stopped = Command::new("herdr")
        .args(["session", "stop", &probe.name, "--json"])
        .output()
        .unwrap();
    drop(terminal);
    support::cleanup_saved(&probe.root);
    if evidence["passed"] != true {
        use std::os::unix::fs::PermissionsExt;
        let diagnostic = std::env::temp_dir().join(format!(
            "kah-native-pane-failure-{}.log",
            std::process::id()
        ));
        fs::copy(probe.root.join("helper.log"), &diagnostic).unwrap();
        fs::set_permissions(&diagnostic, fs::Permissions::from_mode(0o600)).unwrap();
        panic!("native pane journey failed: {}", diagnostic.display());
    }
    fs::write(std::path::PathBuf::from(std::env::var_os("KAH_GATE_EVIDENCE").unwrap()).join("codex-native-pane.json"), serde_json::to_vec_pretty(&json!({"version":String::from_utf8(version.stdout).unwrap().trim(),"fixtureModel":"gpt-6.1-sol","fixtureEffort":"low","sourceDomain":"mine","journey":evidence,"passed":true})).unwrap()).unwrap();
}

#[test]
fn native_pane_probe_entrypoint() {
    let Some(root) = std::env::var_os("KAH_NATIVE_PANE_PROBE") else {
        return;
    };
    let root = std::path::PathBuf::from(root);
    let result = std::panic::catch_unwind(|| support::run(&root));
    let evidence =
        result.unwrap_or_else(|_| json!({"passed":false,"failure":"inspect private helper log"}));
    fs::write(root.join("result.json"), evidence.to_string()).unwrap();
}
