pub use super::herdr_probe::capture;
use super::herdr_probe::{Probe, setup, wait_capture};
use super::{
    history,
    pty::{Terminal, quote},
};
use crate::fixture;
#[path = "public_config.rs"]
mod public_config;
use serde_json::{Value, json};
use std::{
    env, fs,
    path::{Path, PathBuf},
};

pub fn run() {
    run_domain("research");
}

pub fn startup_domains() {
    for domain in ["mine", "research"] {
        run_domain(domain);
    }
}

fn run_domain(domain: &str) {
    let home = PathBuf::from(env::var_os("HOME").unwrap());
    let before = (
        fs::read(home.join(".codex/config.toml")).unwrap(),
        public_config::snapshot(&home.join(".codex/hooks.json")),
    );
    let (_, mut f) = fixture::prepare(&home, domain, false);
    let probe = setup();
    let command = format!(
        "HERDR_CONFIG_PATH={} herdr --session {}",
        quote(probe.root.join("config.toml").to_str().unwrap()),
        quote(&probe.name)
    );
    let mut terminal = Terminal::start(&command, &f.stderr);
    terminal.wait_for("KAH_HERDR_SHELL_READY");
    terminal.send(&(launch_command(&probe, &f.project, domain) + "\r"));
    terminal.wait_for(if domain == "mine" {
        "Folder access"
    } else {
        "Open restricted"
    });
    terminal.send("\r");
    let mut reader = history::connect(&home, &f.project);
    let id = history::session(&mut reader, &f.project, &mut terminal);
    f.sessions.push(id.clone());
    let turn = history::completed_turn(&mut reader, &id, None);
    let capture = check_capture(&probe, &f.project, &id);
    let operations = if domain == "research" {
        crate::checks(&turn)
    } else {
        json!({"status":turn["status"]})
    };
    let startup = super::startup::verify(&home, &f, &id, &terminal);
    let offset = terminal.log_size();
    terminal.send("\x04");
    terminal.wait_for_since("KAH_HERDR_SHELL_READY", offset);
    terminal.send(&format!(
        "test \"$HERDR_ENV\" = 1 && herdr session stop {} --json\r",
        quote(&probe.name)
    ));
    terminal.wait_exit(false);
    drop(terminal);
    drop(reader);
    drop(f);
    drop(probe);
    assert_eq!(
        (
            fs::read(home.join(".codex/config.toml")).unwrap(),
            public_config::snapshot(&home.join(".codex/hooks.json"))
        ),
        before
    );
    save_report(domain, capture, operations, startup);
}

fn save_report(domain: &str, capture: Value, operations: Value, startup: Value) {
    let report = json!({"capture":capture,"nativeTurn":operations,"startup":startup,"globalConfigUnchanged":true,"cleanupComplete":true,"passed":true});
    let output = env::var_os("KAH_GATE_EVIDENCE")
        .map(PathBuf::from)
        .map(|p| p.join(format!("herdr-{domain}.json")))
        .unwrap_or_else(|| {
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../docs/gates/evidence/phase-2/herdr-automated.json")
        });
    fs::write(
        output,
        serde_json::to_string_pretty(&report).unwrap() + "\n",
    )
    .unwrap();
}

fn check_capture(probe: &Probe, project: &Path, id: &str) -> Value {
    assert_eq!(
        fs::read_to_string(project.join("herdr-proof.txt"))
            .unwrap()
            .trim(),
        "HERDR_WORKS"
    );
    let capture = wait_capture(probe);
    assert_eq!(capture["pane"]["agent_session"]["value"], id);
    assert_eq!(capture["pane"]["agent"], "codex");
    assert!(
        capture["states"]
            .as_array()
            .unwrap()
            .contains(&json!("working"))
    );
    capture
}

fn launch_command(probe: &Probe, project: &Path, domain: &str) -> String {
    let executable = env::current_exe().unwrap();
    let kah = Path::new(env!("CARGO_BIN_EXE_kah"));
    let name = project.file_name().unwrap().to_str().unwrap();
    let restricted_prompt = "steer: Use native exec_command once: pwd; test ! -e /home/example/.codex/auth.json && echo HOST_AUTH_HIDDEN; curl --max-time 8 -Is https://example.com; echo CURL_EXIT=$?. Then native apply_patch must create herdr-proof.txt containing HERDR_WORKS. Disposable Herdr test, no other tools, subagents or commits. Report and stop.";
    let prompt = if domain == "research" {
        restricted_prompt
    } else {
        "steer: Use native apply_patch to create herdr-proof.txt containing HERDR_WORKS. Disposable Mine startup test; no other tools, subagents or commits. Report and stop."
    };
    format!(
        "test \"$HERDR_ENV\" = 1 && (KAH_HERDR_CAPTURE={} KAH_HERDR_MARKER={} {} --exact herdr_capture_entrypoint --nocapture > {} 2>&1 &) && {} {domain} {} --model gpt-6.1-sol --effort low -- --no-alt-screen {}",
        quote(probe.root.join("capture.json").to_str().unwrap()),
        quote(project.join("herdr-proof.txt").to_str().unwrap()),
        quote(executable.to_str().unwrap()),
        quote(probe.root.join("helper.log").to_str().unwrap()),
        quote(kah.to_str().unwrap()),
        quote(name),
        quote(prompt)
    )
}
