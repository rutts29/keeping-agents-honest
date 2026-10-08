use super::{claude_login, fixture};
use serde_json::{Value, json};
use std::{fs, process::Command, time::Duration};
pub fn launch(mode: &str) {
    assert_eq!(
        std::env::var("KAH_TRACK2_MAKE_GATE").as_deref(),
        Ok("1"),
        "TRACK2_GATE_ONLY stage=launch: use make gate"
    );
    if claude_login::requires_claude(mode) {
        claude_login::require_ready();
        assert_eq!(
            std::env::var("CLAUDE_CONFIG_DIR").as_deref(),
            Ok(claude_login::CONFIG_DIR),
            "TRACK2_CONFIG_DIR stage=launch"
        );
    }
    let fixture = fixture::Fixture::new(mode);
    let shell = prepare_shell(&fixture);
    let config = fixture.root.join("herdr.toml");
    fs::write(
        &config,
        format!(
            "onboarding=false\n[terminal]\ndefault_shell={}\n",
            json!(shell)
        ),
    )
    .unwrap();
    let command = format!(
        "HERDR_CONFIG_PATH={} herdr --session {}",
        crate::pty::quote(config.to_str().unwrap()),
        crate::pty::quote(&fixture.name)
    );
    let mut terminal =
        crate::pty::Terminal::start_sized(&command, &fixture.root.join("terminal.log"), 200, 50);
    terminal.wait_for("KAH_TRACK2_READY");
    let entry = entrypoint_test(mode);
    terminal.submit(&format!(
        "KAH_TRACK2_ROOT={} KAH_TRACK2_ENTRYPOINT={} {} --exact {} --nocapture > {} 2>&1",
        crate::pty::quote(fixture.root.to_str().unwrap()),
        crate::pty::quote(mode),
        crate::pty::quote(std::env::current_exe().unwrap().to_str().unwrap()),
        entry,
        crate::pty::quote(fixture.root.join("helper.log").to_str().unwrap())
    ));
    let result = wait(&fixture, &mut terminal);
    finish(&fixture, terminal, result, mode);
}
fn finish(fixture: &fixture::Fixture, terminal: crate::pty::Terminal, result: Value, mode: &str) {
    for action in ["stop", "delete"] {
        assert!(
            Command::new("herdr")
                .args(["session", action, &fixture.name, "--json"])
                .output()
                .unwrap()
                .status
                .success(),
            "TRACK2_CLEANUP stage=herdr"
        );
    }
    drop(terminal);
    let trust = super::trust::cleanup_claude(&fixture.home);
    fixture::save(&fixture.root.join("claude-trust-cleanup.json"), &trust);
    let audit = fixture.audit();
    fixture.publish(&json!({"mode":mode,"dry_run":fixture.dry,"result":result,
        "audit":audit,"trust_cleanup":trust,"private_trace":fixture.root,"early_run":mode=="verify","final_acceptance":false,"gate_passed":mode!="verify" && !fixture.dry && result["passed"]==true && audit["passed"]==true && trust["passed"]==true}));
    assert_eq!(audit["passed"], true, "TRACK2_CLEANUP stage=audit: {audit}");
    assert_eq!(
        trust["passed"], true,
        "TRACK2_CLAUDE_TRUST_CLEANUP stage=audit: {trust}"
    );
    assert_eq!(
        result["passed"],
        true,
        "TRACK2_JOURNEY stage=acceptance: {}",
        fixture.root.display()
    );
}

fn entrypoint_test(mode: &str) -> &'static str {
    if mode == "verify" {
        "early_verify_entrypoint"
    } else {
        "private_entrypoint"
    }
}
fn wait(fixture: &fixture::Fixture, terminal: &mut crate::pty::Terminal) -> Value {
    loop {
        terminal.respond_to_cursor_queries();
        if let Ok(bytes) = fs::read(fixture.root.join("result.json")) {
            return serde_json::from_slice(&bytes).unwrap();
        }
        std::thread::sleep(Duration::from_millis(200));
    }
}

fn prepare_shell(fixture: &fixture::Fixture) -> std::path::PathBuf {
    let shell = fixture.root.join("shell");
    let auth_unsets = claude_login::AUTH_ENV
        .iter()
        .map(|key| format!("-u {key}"))
        .collect::<Vec<_>>()
        .join(" ");
    fs::write(&shell, format!("#!/bin/sh\nexec env {} CLAUDE_CONFIG_DIR={} DISABLE_AUTOUPDATER=1 HOME={} XDG_STATE_HOME={} PS1=KAH_TRACK2_READY /bin/bash --noprofile --norc -i\n",
        auth_unsets, crate::pty::quote(claude_login::CONFIG_DIR),crate::pty::quote(fixture.home.to_str().unwrap()),
        crate::pty::quote(fixture.home.join(".local/state").to_str().unwrap()))).unwrap();
    fixture::executable(&shell);
    shell
}
