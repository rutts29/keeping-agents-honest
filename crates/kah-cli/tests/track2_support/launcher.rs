use super::{claude_login, fixture};
use serde_json::{Value, json};
use std::{fs, process::Command, time::Duration};
pub fn launch(mode: &str) {
    assert!(
        std::env::var("KAH_LIVE_CONTROLLER").as_deref() == Ok("1")
            || (mode == "lifecycle"
                && std::env::var("KAH_TRACK2_LIFECYCLE").as_deref() == Ok("1")
                && std::env::var_os("KAH_TRACK2_DRY_RUN").is_some())
            || (mode == "native5e" && std::env::var("KAH_5E_NATIVE_ONLY").as_deref() == Ok("1")),
        "TRACK2_CONTROLLER_ONLY stage=launch: set KAH_LIVE_CONTROLLER=1 on a prepared host"
    );
    if claude_login::requires_claude(mode) {
        claude_login::require_ready();
        assert_eq!(
            std::env::var("CLAUDE_CONFIG_DIR").as_deref(),
            Ok(claude_login::config_dir().to_str().unwrap()),
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
        "KAH_TRACK2_ROOT={} KAH_TRACK2_ENTRYPOINT={} {} --exact {} {} --nocapture > {} 2>&1",
        crate::pty::quote(fixture.root.to_str().unwrap()),
        crate::pty::quote(mode),
        crate::pty::quote(std::env::current_exe().unwrap().to_str().unwrap()),
        entry,
        if mode == "native5e" { "--ignored" } else { "" },
        crate::pty::quote(fixture.root.join("helper.log").to_str().unwrap())
    ));
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        wait(&fixture, &mut terminal, mode)
    }))
    .unwrap_or_else(|_| {
        json!({"passed":false,"reason":"TRACK2_MONITOR_PANIC",
        "stage":"fixture_monitor","retained":fixture.root.join("terminal.log")})
    });
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
    let key = super::jev_key::cleanup(&fixture.home);
    fixture::save(&fixture.root.join("jev-key-cleanup.json"), &key);
    let trust = super::trust::cleanup_claude(&fixture.home);
    fixture::save(&fixture.root.join("claude-trust-cleanup.json"), &trust);
    let audit = fixture.audit();
    let published_audit = if matches!(mode, "native5e" | "lifecycle")
        || std::env::var_os("KAH_PHASE3_B2").is_some()
    {
        let retained = if mode == "native5e" {
            retain_native_exit(fixture, &audit)
        } else {
            fixture::save(&fixture.root.join("cleanup-audit.json"), &audit);
            fixture.root.clone()
        };
        json!({"passed":audit["passed"],"processes":audit["processes"],"jev_key_cleanup":audit["jev_key_cleanup"],
            "credential_contents_opened":audit["claude_login"]["credential_contents_opened"],
            "fixture_claude_credentials_absent":audit["claude_login"]["fixture_claude_credentials_absent"],
            "fixture_attributed_operator_entries":audit["claude_login"]["fixture_attributed_operator_entries"],
            "violations":audit["claude_login"]["violations"],"full_audit":retained.join("cleanup-audit.json")})
    } else {
        audit.clone()
    };
    fixture.publish(&json!({"mode":mode,"dry_run":fixture.dry,"result":result,
        "audit":published_audit,"trust_cleanup":trust,"private_trace":fixture.root,"early_run":mode=="verify" && std::env::var_os("KAH_PHASE3_B2").is_none(),"final_acceptance":false,"gate_passed":!matches!(mode,"verify"|"native5e") && !fixture.dry && result["passed"]==true && audit["passed"]==true && trust["passed"]==true}));
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

fn retain_native_exit(fixture: &fixture::Fixture, audit: &Value) -> std::path::PathBuf {
    let target = std::path::PathBuf::from(std::env::var_os("KAH_GATE_EVIDENCE").unwrap())
        .join(&fixture.name);
    assert!(!target.exists(), "TRACK2_EXISTING_EVIDENCE stage=retain");
    assert!(
        Command::new("cp")
            .args(["-a", "--"])
            .arg(&fixture.root)
            .arg(&target)
            .status()
            .unwrap()
            .success(),
        "TRACK2_RETAIN stage=native_exit"
    );
    fixture::save(&target.join("cleanup-audit.json"), audit);
    target
}

fn entrypoint_test(mode: &str) -> &'static str {
    match mode {
        "verify" | "search" | "relay" | "lifecycle" => "early_verify_entrypoint",
        "native5e" => "mine_claude_native_subagent",
        _ => "private_entrypoint",
    }
}
fn wait(fixture: &fixture::Fixture, terminal: &mut crate::pty::Terminal, mode: &str) -> Value {
    loop {
        terminal.respond_to_cursor_queries();
        if mode != "lifecycle"
            && let Some(stop) = super::bundle_cost::stop(&fixture.root, &fixture.home)
        {
            fixture::save(&fixture.root.join("money-stop.json"), &stop);
            return stop;
        }
        if let Ok(bytes) = fs::read(fixture.root.join("result.json")) {
            return serde_json::from_slice(&bytes).unwrap();
        }
        std::thread::sleep(Duration::from_millis(200));
    }
}

fn prepare_shell(fixture: &fixture::Fixture) -> std::path::PathBuf {
    let shell = fixture.root.join("shell");
    let config = fixture
        .claude_config
        .as_ref()
        .map_or(String::new(), |config| {
            format!(
                " CLAUDE_CONFIG_DIR={}",
                crate::pty::quote(config.to_str().unwrap())
            )
        });
    let auth_unsets = claude_login::AUTH_ENV
        .iter()
        .map(|key| format!("-u {key}"))
        .collect::<Vec<_>>()
        .join(" ");
    fs::write(&shell, format!("#!/bin/sh\nexec env {}{} DISABLE_AUTOUPDATER=1 HOME={} XDG_STATE_HOME={} PS1=KAH_TRACK2_READY /bin/bash --noprofile --norc -i\n",
        auth_unsets, config,crate::pty::quote(fixture.home.to_str().unwrap()),
        crate::pty::quote(fixture.home.join(".local/state").to_str().unwrap()))).unwrap();
    fixture::executable(&shell);
    shell
}
