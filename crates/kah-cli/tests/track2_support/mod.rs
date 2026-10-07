mod config_audit;
mod fixture;
mod journey;
mod perturb;
mod proof;
mod proof_contract;
mod proof_hooks;
mod proof_inputs;
mod proof_journey;
mod proof_probe;
mod proof_replay;
mod scenarios;
mod startup;
mod telemetry;
mod trust;
mod usage;
pub use perturb::write_run_store;
use serde_json::{Value, json};
use std::{fs, path::PathBuf, process::Command, time::Duration};

pub fn launch(mode: &str) {
    assert_eq!(
        std::env::var("KAH_TRACK2_MAKE_GATE").as_deref(),
        Ok("1"),
        "TRACK2_GATE_ONLY stage=launch: use make gate"
    );
    let fixture = fixture::Fixture::new(mode);
    let shell = fixture.root.join("shell");
    let config = fixture.root.join("herdr.toml");
    fs::write(&shell, format!("#!/bin/sh\nexec env HOME={} XDG_STATE_HOME={} PS1=KAH_TRACK2_READY /bin/bash --noprofile --norc -i\n",
        crate::pty::quote(fixture.home.to_str().unwrap()),
        crate::pty::quote(fixture.home.join(".local/state").to_str().unwrap()))).unwrap();
    fixture::executable(&shell);
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
    terminal.submit(&format!("KAH_TRACK2_ROOT={} KAH_TRACK2_ENTRYPOINT={} {} --exact private_entrypoint --nocapture > {} 2>&1",
        crate::pty::quote(fixture.root.to_str().unwrap()),crate::pty::quote(mode),
        crate::pty::quote(std::env::current_exe().unwrap().to_str().unwrap()),
        crate::pty::quote(fixture.root.join("helper.log").to_str().unwrap())));
    let result = wait(&fixture, &mut terminal);
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
    let audit = fixture.audit();
    fixture.publish(&json!({"mode":mode,"dry_run":fixture.dry,"result":result,
        "audit":audit,"private_trace":fixture.root,"gate_passed":!fixture.dry && result["passed"]==true && audit["passed"]==true}));
    assert_eq!(audit["passed"], true, "TRACK2_CLEANUP stage=audit: {audit}");
    assert_eq!(
        result["passed"],
        true,
        "TRACK2_JOURNEY stage=acceptance: {}",
        fixture.root.display()
    );
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

pub fn entrypoint(mode: &str) {
    let root = PathBuf::from(std::env::var_os("KAH_TRACK2_ROOT").unwrap());
    let run = std::panic::catch_unwind(|| journey::run(&root, mode));
    let result=run.unwrap_or_else(|_| json!({"passed":false,"reason":"TRACK2_PANIC","stage":"journey","retained":root.join("helper.log")}));
    fixture::save(&root.join("result.json"), &result);
}
