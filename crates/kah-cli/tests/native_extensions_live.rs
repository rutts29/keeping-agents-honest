#[path = "journeys/fixture.rs"]
mod fixture;
#[allow(dead_code)]
#[path = "terminal_support/history.rs"]
mod history;
#[allow(dead_code)]
#[path = "terminal_support/processes.rs"]
mod processes;
#[allow(dead_code)]
#[path = "terminal_support/pty.rs"]
mod pty;

use kah_codex::overlay::{self, OwnedHook};
use kah_sandbox::{Executor, LaunchOptions, current_uid, publish_binary};
use pty::{Terminal, quote};
use serde_json::json;
use std::{fs, path::PathBuf, process::Command};

#[test]
#[ignore = "real Codex TUI with native remote MCP and a sandboxed command hook"]
fn native_remote_mcp_and_hook_in_gvisor() {
    let home = PathBuf::from(std::env::var_os("HOME").unwrap());
    let before = globals(&home);
    let version = Command::new(home.join(".local/bin/codex"))
        .arg("--version")
        .output()
        .unwrap();
    let (launch, mut f) = fixture::prepare(&home, "research", false);
    configure(&home, &launch, &f);
    verify_catalog(&home, &f);
    let prompt = "steer: Invoke the fixture MCP probe tool once, using native tool discovery if needed. Then use native apply_patch to create native-proof.txt containing NATIVE_WORKS. No other tools, subagents, commits or installs. Report the MCP result and stop.";
    let mut command = launch.command(false);
    command.args(["--no-alt-screen", prompt]);
    let mut terminal = Terminal::start(&shell(&command), &f.stderr);
    terminal.wait_for("Open restricted");
    terminal.send("\r");
    let mut reader = history::connect(&home, &f.project);
    let id = history::session(&mut reader, &f.project, &mut terminal);
    f.sessions.push(id.clone());
    let turn = history::completed_turn(&mut reader, &id, None);
    let proof: serde_json::Value =
        serde_json::from_slice(&fs::read(f.project.join("mcp-proof.json")).unwrap()).unwrap();
    assert_eq!(proof["cwd"], f.project.to_str().unwrap());
    assert_eq!(proof["hostCanaryHidden"], true);
    assert_eq!(proof["hostLoginHidden"], true);
    assert_eq!(proof["networkExit"], 6);
    assert_eq!(
        fs::read_to_string(f.project.join("hook-proof.txt")).unwrap(),
        "HOOK_GVISOR"
    );
    assert_eq!(
        fs::read_to_string(f.project.join("native-proof.txt"))
            .unwrap()
            .trim(),
        "NATIVE_WORKS"
    );
    let mcp = turn["items"]
        .as_array()
        .unwrap()
        .iter()
        .any(|v| v["type"] == "mcpToolCall");
    assert!(mcp, "no native MCP call was recorded");
    terminal.close();
    drop(terminal);
    drop(reader);
    drop(f);
    assert_eq!(before, globals(&home));
    let evidence = PathBuf::from(std::env::var_os("KAH_GATE_EVIDENCE").unwrap());
    fs::write(
        evidence.join("native-extensions.json"),
        serde_json::to_vec_pretty(&json!({
            "version":String::from_utf8(version.stdout).unwrap().trim(),"domain":"research-offline",
            "fixtureModel":"gpt-6-sol","fixtureEffort":"low","nativeMcpCall":mcp,"mcpProof":proof,
            "commandHookInGvisor":true,"nativeEdit":true,"globalConfigUnchanged":true,"passed":true
        }))
        .unwrap(),
    )
    .unwrap();
}

fn globals(home: &std::path::Path) -> (Vec<u8>, Vec<u8>) {
    (
        fs::read(home.join(".codex/config.toml")).unwrap(),
        fs::read(home.join(".codex/hooks.json")).unwrap(),
    )
}

fn verify_catalog(home: &std::path::Path, f: &fixture::Fixture) {
    let mut native = kah_codex::control::Control::connect(
        &home.join(".local/bin/codex"),
        &f.overlay,
        &f.project,
        std::time::Duration::from_secs(45),
    )
    .unwrap();
    let result = native
        .request("mcpServerStatus/list", json!({"serverName":"fixture"}))
        .unwrap();
    assert!(
        result["data"].as_array().unwrap().iter().any(|v| v["tools"]
            .as_object()
            .is_some_and(|t| t.contains_key("probe"))),
        "fixture MCP catalog: {result}"
    );
}

fn configure(home: &std::path::Path, launch: &kah_cli::Launch, f: &fixture::Fixture) {
    let script = include_str!("native_extensions_support/server.js");
    fs::write(f.project.join("fixture-server.js"), script).unwrap();
    overlay::write_settings(
        &f.overlay,
        json!({"mcp_servers":{"fixture":{
            "command":"/usr/bin/node","args":[f.project.join("fixture-server.js"),f.canary],
            "cwd":f.project,"environment_id":launch.environment_id,"startup_timeout_sec":30
        }}}),
    )
    .unwrap();
    let binary = publish_binary(
        &home.join(".local/bin/codex"),
        &PathBuf::from(format!("/var/tmp/kah-runtime-{}", current_uid().unwrap())),
    )
    .unwrap();
    let image = fs::read_to_string("/usr/local/share/sbx/image-ref").unwrap();
    let mut executor = Executor::new(
        &launch.project,
        LaunchOptions::default(),
        &binary,
        image.trim(),
        kah_run::runtime::service_uid().unwrap(),
    )
    .unwrap();
    executor.args.truncate(executor.args.len() - 4);
    let check = format!(
        "test ! -e {} && test ! -e /home/example/.codex/auth.json && printf HOOK_GVISOR > hook-proof.txt",
        quote(f.canary.to_str().unwrap())
    );
    executor.args.extend(["/bin/sh".into(), "-c".into(), check]);
    let command = std::iter::once("/usr/bin/docker".to_owned())
        .chain(executor.args)
        .map(|s| quote(&s))
        .collect::<Vec<_>>()
        .join(" ");
    overlay::install_owned_hooks(
        &home.join(".local/bin/codex"),
        &f.overlay,
        &f.project,
        &[OwnedHook {
            event: "SessionStart",
            api_event: "sessionStart",
            command,
            timeout: 30,
        }],
    )
    .unwrap();
}

fn shell(command: &Command) -> String {
    let mut parts = vec!["env".to_owned()];
    for (key, _) in command.get_envs().filter(|(_, v)| v.is_none()) {
        parts.extend(["-u".into(), quote(key.to_str().unwrap())]);
    }
    for (key, value) in command.get_envs() {
        if let Some(value) = value {
            parts.push(quote(&format!(
                "{}={}",
                key.to_str().unwrap(),
                value.to_str().unwrap()
            )));
        }
    }
    parts.push(quote(command.get_program().to_str().unwrap()));
    parts.extend(command.get_args().map(|a| quote(a.to_str().unwrap())));
    format!(
        "cd {} && {}",
        quote(command.get_current_dir().unwrap().to_str().unwrap()),
        parts.join(" ")
    )
}
