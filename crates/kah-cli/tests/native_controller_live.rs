#[allow(dead_code)]
#[path = "terminal_support/herdr_probe.rs"]
mod herdr_probe;
mod native_controller_support;
#[path = "../src/privacy.rs"]
mod privacy;
#[allow(dead_code)]
#[path = "terminal_support/processes.rs"]
mod processes;
#[allow(dead_code)]
#[path = "terminal_support/pty.rs"]
mod pty;
#[path = "relay_support/review_trace.rs"]
mod review_trace;
#[path = "terminal_support/version.rs"]
mod version;
use serde_json::json;
use std::{fs, path::PathBuf, process::Command, time::Duration};

#[test]
#[ignore = "gVisor checkpoint/import/verification path diagnostic; no model call"]
fn verification_uses_imported_commit() {
    native_controller_support::verification_uses_imported_commit();
}

#[test]
#[ignore = "public kah run: full Codex and Claude sessions, verified completion in Herdr"]
fn native_run_journeys() {
    for agent in ["claude", "codex"] {
        if std::env::var("KAH_GATE_AGENT").is_ok_and(|selected| selected != agent) {
            continue;
        }
        journey(agent);
    }
}
fn journey(agent: &str) {
    let version = version::Version::capture(agent);
    let probe = herdr_probe::setup();
    enable_toasts(&probe.root);
    let command = format!(
        "HERDR_CONFIG_PATH={} herdr --session {}",
        pty::quote(probe.root.join("config.toml").to_str().unwrap()),
        pty::quote(&probe.name)
    );
    let mut terminal = pty::Terminal::start(&command, &probe.root.join("terminal.log"));
    terminal.wait_for("KAH_HERDR_SHELL_READY");
    terminal.submit(&format!("env -u CODEX_THREAD_ID KAH_NATIVE_RUN_PROBE={} KAH_NATIVE_RUN_AGENT={} {} --exact native_run_probe_entrypoint --nocapture > {} 2>&1", pty::quote(probe.root.to_str().unwrap()), pty::quote(agent), pty::quote(std::env::current_exe().unwrap().to_str().unwrap()), pty::quote(probe.root.join("helper.log").to_str().unwrap())));
    let (deadline, secs) = herdr_probe::journey_deadline(1800);
    let evidence = loop {
        terminal.respond_to_cursor_queries();
        if let Ok(bytes) = fs::read(probe.root.join("result.json")) {
            break serde_json::from_slice::<serde_json::Value>(&bytes).unwrap();
        }
        if std::time::Instant::now() >= deadline {
            break herdr_probe::timeout_evidence("native_run", secs);
        }
        std::thread::sleep(Duration::from_millis(200));
    };
    let notification_rendered = terminal.output().contains("kah run needs attention");
    // A failed archive in preserve_failure used to hide this diagnostic.
    if evidence["passed"] != true {
        eprintln!("PRIMARY FAILURE (reported before cleanup): {evidence}");
    }
    let diagnostic = preserve_failure(&probe, agent, &evidence);
    let _ = Command::new("herdr")
        .args(["session", "stop", &probe.name, "--json"])
        .output();
    drop(terminal);
    // A cleanup panic must not hide the journey's own failure.
    let cleanup = std::panic::catch_unwind(|| native_controller_support::cleanup(&probe.root));
    if evidence["passed"] != true {
        panic!(
            "native run journey failed: {}",
            diagnostic.unwrap().display()
        );
    }
    if let Err(panic) = cleanup {
        std::panic::resume_unwind(panic);
    }
    if evidence["consentRequired"] != false {
        assert!(
            notification_rendered,
            "Herdr must render the consent notification"
        );
    }
    version.verify(&evidence["version"]);
    fs::write(
        PathBuf::from(std::env::var_os("KAH_GATE_EVIDENCE").unwrap()).join(evidence_name(agent)),
        serde_json::to_vec_pretty(&evidence).unwrap(),
    )
    .unwrap();
}
fn preserve_failure(
    probe: &herdr_probe::Probe,
    agent: &str,
    evidence: &serde_json::Value,
) -> Option<PathBuf> {
    use std::os::unix::fs::PermissionsExt;
    if evidence["passed"] == true {
        return None;
    }
    preserve_review(&probe.root);
    let helper = std::env::temp_dir().join(format!(
        "kah-native-run-failure-{agent}-{}.log",
        std::process::id()
    ));
    for (source, target) in [
        ("helper.log", helper.clone()),
        ("terminal.log", helper.with_extension("terminal.log")),
    ] {
        fs::copy(probe.root.join(source), &target).unwrap();
        fs::set_permissions(target, fs::Permissions::from_mode(0o600)).unwrap();
    }
    Some(helper)
}
fn preserve_review(root: &std::path::Path) {
    let saved: serde_json::Value =
        serde_json::from_slice(&fs::read(root.join("cleanup.json")).unwrap()).unwrap();
    let base = kah_core::run_store::RunStore::default_state_root().unwrap();
    let record =
        kah_run::store::Store::read(&base.join("runs"), saved["run"].as_str().unwrap()).unwrap();
    if let Some(session) = record.native.as_ref().and_then(|n| n.session.as_deref())
        && let Ok(Some(state)) = kah_cli::observer_review::state::Store::read(session)
    {
        let home = PathBuf::from(std::env::var_os("HOME").unwrap());
        review_trace::archive(&home, &state).unwrap();
    }
}
fn evidence_name(agent: &str) -> String {
    let kind = if let Ok(trial) = std::env::var("KAH_GATE_COMPLETION_TRIAL") {
        format!("completion-{trial}")
    } else if std::env::var_os("KAH_GATE_PRIVATE_NOTES").is_some() {
        "private-notes".into()
    } else if let Ok(mode) = std::env::var("KAH_GATE_LIMIT") {
        format!("limit-{mode}")
    } else if std::env::var_os("KAH_GATE_EMPTY").is_some() {
        "empty".into()
    } else if std::env::var_os("KAH_GATE_USAGE").is_some() {
        "usage".into()
    } else if std::env::var_os("KAH_GATE_RECOVERY").is_some() {
        if std::env::var_os("KAH_GATE_RECOVERY_STEER").is_some() {
            "recovery-steered".into()
        } else {
            "recovery".into()
        }
    } else {
        "run".into()
    };
    format!("native-{kind}-{agent}.json")
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
#[test]
fn owned_fixture_cleanup_entrypoint() {
    if let Some(root) = std::env::var_os("KAH_NATIVE_CLEANUP_PROBE") {
        native_controller_support::cleanup(&PathBuf::from(root));
    }
}

fn enable_toasts(root: &std::path::Path) {
    let config = root.join("config.toml");
    fs::write(
        &config,
        format!(
            "{}\n[ui.toast]\ndelivery='herdr'\n",
            fs::read_to_string(&config).unwrap()
        ),
    )
    .unwrap();
}
