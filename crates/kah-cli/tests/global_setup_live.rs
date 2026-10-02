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
use kah_codex::{control::Control, overlay};
use pty::Terminal;
use serde_json::{Value, json};
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
    time::Duration,
};

#[test]
#[ignore = "full native global Codex setup and subagent in Research and Untrusted"]
fn codex_global_setup_in_both_restricted_domains() {
    for domain in ["research", "untrusted"] {
        journey(domain);
    }
}
fn journey(domain: &str) {
    let home = PathBuf::from(std::env::var_os("HOME").unwrap());
    let before = fs::read(home.join(".codex/config.toml")).unwrap();
    let hooks_before = fs::read(home.join(".codex/hooks.json")).unwrap();
    let version = Command::new(home.join(".local/bin/codex"))
        .arg("--version")
        .output()
        .unwrap();
    let (launch, mut f) = fixture::prepare(&home, domain, false);
    let inventory = inventory(&home, &f);
    fs::write(
        f.project.join("fixture-server.js"),
        include_str!("native_extensions_support/server.js"),
    )
    .unwrap();
    overlay::write_settings(&f.overlay,json!({"mcp_servers":{"fixture":{"command":"/usr/bin/node","args":[f.project.join("fixture-server.js"),f.canary],"cwd":f.project,"environment_id":"kah-mcp-online"}}})).unwrap();
    let skill = home.join(".agents/skills/ponytail/SKILL.md");
    let task = format!(
        "steer: Disposable full-setup journey. Read the installed ponytail skill at {} and apply its minimal-change guidance. Invoke fixture MCP probe once (native discovery allowed). Use native apply_patch to write parent.txt with PARENT_OK. Spawn exactly one native subagent for this authorized test only: have it read the same global skill, run native exec_command `test ! -e {} && echo CHILD_HOST_HIDDEN`, and use native apply_patch to write child.txt with CHILD_OK. Wait for that child, report its result, and stop. No other subagents, installs, commits or permission workarounds.",
        skill.display(),
        f.canary.display()
    );
    let mut command = launch.command(false);
    command.args(["--no-alt-screen", &task]);
    let mut terminal = Terminal::start(&pty::command_line(&command), &f.stderr);
    terminal.wait_for("Open restricted");
    terminal.send("\r");
    let mut reader = history::connect(&home, &f.project);
    let id = history::session(&mut reader, &f.project, &mut terminal);
    f.sessions.push(id.clone());
    let turn = history::completed_turn(&mut reader, &id, None);
    let child = child_proof(&mut reader, &turn, &mut f);
    for (file, marker) in [("parent.txt", "PARENT_OK"), ("child.txt", "CHILD_OK")] {
        assert_eq!(
            fs::read_to_string(f.project.join(file)).unwrap().trim(),
            marker
        );
    }
    let proof: Value =
        serde_json::from_slice(&fs::read(f.project.join("mcp-proof.json")).unwrap()).unwrap();
    assert_eq!(proof["networkExit"], 0);
    assert_eq!(proof["hostLoginHidden"], true);
    assert!(
        !terminal.output().contains("No such file or directory"),
        "setup path missing"
    );
    drop(terminal);
    drop(reader);
    drop(f);
    assert!(
        before == fs::read(home.join(".codex/config.toml")).unwrap(),
        "global config changed"
    );
    assert!(
        hooks_before == fs::read(home.join(".codex/hooks.json")).unwrap(),
        "global hooks changed"
    );
    let evidence = PathBuf::from(std::env::var_os("KAH_GATE_EVIDENCE").unwrap());
    fs::write(evidence.join(format!("codex-global-{domain}.json")),serde_json::to_vec_pretty(&json!({"version":String::from_utf8(version.stdout).unwrap().trim(),"fixtureModel":"gpt-6.1-sol","fixtureEffort":"low","domain":domain,"inventory":inventory,"mcp":proof,"child":child,"nativeEdits":true,"globalConfigUnchanged":true,"passed":true})).unwrap()).unwrap();
}

fn child_proof(reader: &mut Control, turn: &Value, f: &mut fixture::Fixture) -> Value {
    let ids = child_ids(turn);
    assert_eq!(ids.len(), 1, "expected exactly one native child");
    let id = ids.first().unwrap();
    f.sessions.push(id.clone());
    let child = history::completed_turn(reader, id, None);
    let commands = child["items"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|item| item["type"] == "commandExecution")
        .collect::<Vec<_>>();
    assert!(
        commands.iter().any(|item| item["aggregatedOutput"]
            .as_str()
            .is_some_and(|s| s.contains("CHILD_HOST_HIDDEN"))),
        "child command did not prove isolation"
    );
    assert!(
        commands.iter().any(|item| item["command"]
            .as_str()
            .is_some_and(|s| s.contains("ponytail/SKILL.md"))),
        "child did not read the global skill"
    );
    json!({"nativeChild":true,"globalSkillRead":true,"hostCanaryHidden":true})
}
fn child_ids(turn: &Value) -> std::collections::BTreeSet<String> {
    let mut ids = std::collections::BTreeSet::new();
    for item in turn["items"].as_array().unwrap() {
        if item["type"] == "subAgentActivity" {
            if let Some(id) = item["agentThreadId"].as_str() {
                ids.insert(id.into());
            }
        } else if item["type"] == "collabAgentToolCall" {
            ids.extend(
                item["receiverThreadIds"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(Value::as_str)
                    .map(str::to_owned),
            );
        }
    }
    ids
}

#[test]
fn child_identity_comes_from_native_activity_not_parent_text() {
    let turn = json!({"items":[{"type":"subAgentActivity","kind":"started","agentThreadId":"child-id"},{"type":"subAgentActivity","kind":"completed","agentThreadId":"child-id"},{"type":"agentMessage","text":"another child-id claimed here"}]});
    assert_eq!(
        child_ids(&turn),
        std::collections::BTreeSet::from(["child-id".into()])
    );
}
fn inventory(home: &Path, f: &fixture::Fixture) -> Value {
    let original: toml::Value =
        toml::from_str(&fs::read_to_string(home.join(".codex/config.toml")).unwrap()).unwrap();
    let actual: toml::Value =
        toml::from_str(&fs::read_to_string(f.overlay.join("config.toml")).unwrap()).unwrap();
    assert!(
        actual.get("plugins") == original.get("plugins"),
        "plugin settings differ from the user's setup"
    );
    for name in [
        "skills",
        "plugins",
        "packages",
        "rules",
        "agents",
        "AGENTS.md",
        "AGENTS.override.md",
    ] {
        if home.join(".codex").join(name).exists() {
            assert_eq!(
                fs::canonicalize(f.overlay.join(name)).unwrap(),
                fs::canonicalize(home.join(".codex").join(name)).unwrap()
            );
        }
    }
    let hooks: Value =
        serde_json::from_slice(&fs::read(f.overlay.join("hooks.json")).unwrap()).unwrap();
    assert!(
        hooks.to_string().contains("--sandbox-shell"),
        "global command hook was not routed"
    );
    let mut control = Control::connect(
        &home.join(".local/bin/codex"),
        &f.overlay,
        &f.project,
        Duration::from_secs(30),
    )
    .unwrap();
    let skills = control
        .request(
            "skills/list",
            json!({"cwds":[f.project],"forceReload":true}),
        )
        .unwrap();
    assert!(
        skills.to_string().contains("ponytail"),
        "real global skill not discovered"
    );
    let plugins = control
        .request(
            "plugin/list",
            json!({"cwds":[f.project],"forceRefetch":false,"marketplaceKinds":["local"]}),
        )
        .unwrap();
    json!({"globalSkillDiscovered":true,"nativePluginCatalogRead":plugins.is_object(),"userPluginSettingsPreserved":true,"globalHooksRouted":true,"globalInstructionsPresent":home.join(".codex/AGENTS.md").is_file()})
}
