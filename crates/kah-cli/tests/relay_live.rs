#[allow(dead_code)]
mod claude_support;
#[allow(dead_code)]
#[path = "journeys/fixture.rs"]
mod fixture;
#[allow(dead_code)]
#[path = "terminal_support/herdr_probe.rs"]
mod herdr_probe;
#[allow(dead_code, unused_imports)]
mod native_controller_support;
#[allow(dead_code)]
#[path = "terminal_support/processes.rs"]
mod processes;
#[allow(dead_code)]
#[path = "terminal_support/pty.rs"]
mod pty;
mod relay_support;
#[path = "terminal_support/version.rs"]
mod version;
use serde_json::{Value, json};
use std::{
    fs,
    path::PathBuf,
    process::Command,
    time::{Duration, Instant},
};

#[test]
#[ignore = "two-way native relay and both native worker dispatches in a private Herdr server"]
fn relay_and_dispatch() {
    let versions = [
        version::Version::capture("codex"),
        version::Version::capture("claude"),
    ];
    let probe = herdr_probe::setup();
    let command = format!(
        "HERDR_CONFIG_PATH={} herdr --session {}",
        pty::quote(probe.root.join("config.toml").to_str().unwrap()),
        pty::quote(&probe.name)
    );
    let mut terminal = pty::Terminal::start(&command, &probe.root.join("terminal.log"));
    terminal.wait_for("KAH_HERDR_SHELL_READY");
    terminal.submit(&format!(
        "KAH_RELAY_PROBE={} {} --exact relay_probe_entrypoint --nocapture > {} 2>&1",
        pty::quote(probe.root.to_str().unwrap()),
        pty::quote(std::env::current_exe().unwrap().to_str().unwrap()),
        pty::quote(probe.root.join("helper.log").to_str().unwrap())
    ));
    let deadline = Instant::now() + Duration::from_secs(900);
    let evidence = loop {
        terminal.respond_to_cursor_queries();
        if let Ok(bytes) = fs::read(probe.root.join("result.json")) {
            break serde_json::from_slice::<Value>(&bytes).unwrap();
        }
        if Instant::now() >= deadline {
            break json!({"passed":false,"failure":"relay helper timed out"});
        }
        std::thread::sleep(Duration::from_millis(200));
    };
    if evidence["passed"] != true {
        let path = PathBuf::from(format!("/tmp/kah-relay-failure-{}.log", std::process::id()));
        fs::copy(probe.root.join("helper.log"), &path).unwrap();
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
        if probe.root.join("submit.json").exists() {
            let trace = path.with_extension("submit.json");
            fs::copy(probe.root.join("submit.json"), &trace).unwrap();
            fs::set_permissions(trace, fs::Permissions::from_mode(0o600)).unwrap();
        }
        eprintln!("relay failure: {}", path.display());
    }
    Command::new("herdr")
        .args(["session", "stop", &probe.name, "--json"])
        .output()
        .unwrap();
    drop(terminal);
    native_controller_support::cleanup(&probe.root);
    assert_eq!(evidence["passed"], true, "{evidence}");
    for (n, agent) in ["codex", "claude"].iter().enumerate() {
        versions[n].verify(&evidence["versions"][agent]);
    }
    fs::write(
        PathBuf::from(std::env::var_os("KAH_GATE_EVIDENCE").unwrap()).join("relay-dispatch.json"),
        serde_json::to_vec_pretty(&evidence).unwrap(),
    )
    .unwrap();
}

#[test]
fn relay_probe_entrypoint() {
    let Some(root) = std::env::var_os("KAH_RELAY_PROBE").map(PathBuf::from) else {
        return;
    };
    let result = std::panic::catch_unwind(|| relay_support::run(&root));
    let result = result.unwrap_or_else(|_| json!({"passed":false}));
    fs::write(
        root.join("result.json"),
        serde_json::to_vec_pretty(&result).unwrap(),
    )
    .unwrap();
}
